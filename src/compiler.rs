use atomic_refcell::{AtomicRefCell, AtomicRefMut};
use audio_graph::DelayCompInfo;
use basedrop::{Handle, Shared, SharedCell};
use rusty_daw_core::block_buffer::{MonoBlockBuffer, StereoBlockBuffer};
use rusty_daw_core::SampleRate;
use smallvec::{smallvec, SmallVec};

use crate::task::{AudioGraphNodeTask, MimicProcessReplacingTask};

use super::node::sample_delay::{MonoSampleDelayNode, StereoSampleDelayNode};
use super::node::sum::{MonoSumNode, StereoSumNode};
use super::resource_pool::{
    DebugBufferID, DebugNodeID, DelayCompNodeKey, GraphResourcePool, SumNodeKey,
};
use super::task::AudioGraphTask;
use super::{
    AudioGraphNode, GraphInterface, MonoProcBuffers, MonoProcBuffersMut, NodeRef, PortIdent,
    PortType, ProcBuffers, Schedule, StereoProcBuffers, StereoProcBuffersMut,
    SMALLVEC_ALLOC_BUFFERS, SMALLVEC_ALLOC_MPR_BUFFERS,
};

// Beware: this is one hefty boi of a function
// TODO: errors and reverting to previous working state
pub(crate) fn compile_graph<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>(
    graph: &mut GraphInterface<GlobalData, MAX_BLOCKSIZE>,
) -> Result<(), ()> {
    let GraphInterface {
        shared_graph_state: _,
        resource_pool,
        graph_state,

        sample_rate: _,
        coll_handle,

        root_node_ref: _,
        _root_node_handle: _,
        next_delay_comp_node_id,
        next_sum_node_id,
    } = graph;

    let mut master_out_buffer = None;

    // Flag all delay comp and sum nodes as unused so we can detect which ones should be
    // removed later.
    resource_pool.flag_unused();

    // Used to detect if there are more buffers allocated than needed.
    let mut max_mono_block_buffer_id = 0;
    let mut max_stereo_block_buffer_id = 0;
    let mut max_temp_mono_block_buffer_id = 0;
    let mut max_temp_stereo_block_buffer_id = 0;

    // TODO: We will need to ensure that none of these buffers overlap when we start using
    // a multi-threaded schedule.
    let mut next_temp_mono_block_buffer;
    let mut next_temp_stereo_block_buffer;

    let mut root_node_scheduled = false;

    let graph_schedule = graph_state.graph.compile();

    // TODO: Particuarly with this specific Vec, it could be beneficial to recycle the
    // allocated memory of the old schedule (once the rt thread is finished with it). The
    // entire schedule is recreated every time the graph changes, so this optimization
    // could be worth looking into.
    //
    // It would also be cool if we could also somehow recycle the tasks of nodes with a
    // particuarly large number of assigned buffers (i.e. a mixer), but I imagine this would
    // be hard to pull off with the current setup. So I wouldn't worry unless it becomes a
    // serious performance problem in practice.
    let mut tasks =
        Vec::<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>::with_capacity(graph_schedule.len() * 2);

    // Insert a mono delay comp node into the schedule. This returns the ID of the temp buffer used.
    let insert_mono_delay_comp_node =
        |tasks: &mut Vec<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>,
         resource_pool: &mut GraphResourcePool<GlobalData, MAX_BLOCKSIZE>,
         next_temp_mono_block_buffer: &mut usize,
         next_delay_comp_node_id: &mut u64,
         delay_comp_info: &DelayCompInfo<NodeRef, PortIdent>,
         node_id: NodeRef,
         port_id: PortIdent,
         buffer_id: usize|
         -> usize {
            let delayed_buffer =
                resource_pool.get_temp_mono_audio_block_buffer(*next_temp_mono_block_buffer);
            *next_temp_mono_block_buffer += 1;

            let src_node_id: usize = delay_comp_info.source_node.into();
            let dst_node_id: usize = node_id.into();
            let key = DelayCompNodeKey {
                src_node_id: src_node_id as u32,
                src_node_port: delay_comp_info.source_port,
                dst_node_id: dst_node_id as u32,
                dst_node_port: port_id,
            };

            let new_delay = delay_comp_info.delay as u32;

            let delay_node =
                if let Some(old_delay_node) = resource_pool.delay_comp_nodes.get_mut(&key) {
                    // Mark that this node is still being used.
                    old_delay_node.2 = true;

                    if old_delay_node.1 == new_delay {
                        // Delay has not changed, just return the existing node.
                        Shared::clone(&old_delay_node.0)
                    } else {
                        // Delay has changed, replace the node.
                        let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                            Box::new(MonoSampleDelayNode::new(new_delay));
                        let new_node = Shared::new(
                            coll_handle,
                            (
                                AtomicRefCell::new(new_delay_node),
                                DebugNodeID::DelayComp(*next_delay_comp_node_id),
                            ),
                        );
                        *next_delay_comp_node_id += 1;

                        old_delay_node.0 = Shared::clone(&new_node);
                        old_delay_node.1 = new_delay;

                        new_node
                    }
                } else {
                    let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                        Box::new(MonoSampleDelayNode::new(new_delay));
                    let new_node = Shared::new(
                        coll_handle,
                        (
                            AtomicRefCell::new(new_delay_node),
                            DebugNodeID::DelayComp(*next_delay_comp_node_id),
                        ),
                    );
                    *next_delay_comp_node_id += 1;

                    let _ = resource_pool
                        .delay_comp_nodes
                        .insert(key, (Shared::clone(&new_node), new_delay, true));

                    new_node
                };

            tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                node: delay_node,
                proc_buffers: ProcBuffers {
                    mono_through: MonoProcBuffersMut::new(smallvec![]),
                    indep_mono_in: MonoProcBuffers::new(smallvec![(
                        resource_pool.get_mono_audio_block_buffer(buffer_id),
                        0,
                    )]),
                    indep_mono_out: MonoProcBuffersMut::new(smallvec![(delayed_buffer, 0)]),
                    stereo_through: StereoProcBuffersMut::new(smallvec![]),
                    indep_stereo_in: StereoProcBuffers::new(smallvec![]),
                    indep_stereo_out: StereoProcBuffersMut::new(smallvec![]),
                },
            }));

            *next_temp_mono_block_buffer - 1
        };

    // Insert a stereo delay comp node into the schedule. This returns the ID of the temp buffer used.
    let insert_stereo_delay_comp_node =
        |tasks: &mut Vec<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>,
         resource_pool: &mut GraphResourcePool<GlobalData, MAX_BLOCKSIZE>,
         next_temp_stereo_block_buffer: &mut usize,
         next_delay_comp_node_id: &mut u64,
         delay_comp_info: &DelayCompInfo<NodeRef, PortIdent>,
         node_id: NodeRef,
         port_id: PortIdent,
         buffer_id: usize|
         -> usize {
            let delayed_buffer =
                resource_pool.get_temp_stereo_audio_block_buffer(*next_temp_stereo_block_buffer);
            *next_temp_stereo_block_buffer += 1;

            let src_node_id: usize = delay_comp_info.source_node.into();
            let dst_node_id: usize = node_id.into();
            let key = DelayCompNodeKey {
                src_node_id: src_node_id as u32,
                src_node_port: delay_comp_info.source_port,
                dst_node_id: dst_node_id as u32,
                dst_node_port: port_id,
            };

            let new_delay = delay_comp_info.delay as u32;

            let delay_node =
                if let Some(old_delay_node) = resource_pool.delay_comp_nodes.get_mut(&key) {
                    // Mark that this node is still being used.
                    old_delay_node.2 = true;

                    if old_delay_node.1 == new_delay {
                        // Delay has not changed, just return the existing node.
                        Shared::clone(&old_delay_node.0)
                    } else {
                        // Delay has changed, replace the node.
                        let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                            Box::new(StereoSampleDelayNode::new(new_delay));
                        let new_node = Shared::new(
                            coll_handle,
                            (
                                AtomicRefCell::new(new_delay_node),
                                DebugNodeID::DelayComp(*next_delay_comp_node_id),
                            ),
                        );
                        *next_delay_comp_node_id += 1;

                        old_delay_node.0 = Shared::clone(&new_node);
                        old_delay_node.1 = new_delay;

                        new_node
                    }
                } else {
                    let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                        Box::new(StereoSampleDelayNode::new(new_delay));
                    let new_node = Shared::new(
                        coll_handle,
                        (
                            AtomicRefCell::new(new_delay_node),
                            DebugNodeID::DelayComp(*next_delay_comp_node_id),
                        ),
                    );
                    *next_delay_comp_node_id += 1;

                    let _ = resource_pool
                        .delay_comp_nodes
                        .insert(key, (Shared::clone(&new_node), new_delay, true));

                    new_node
                };

            tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                node: delay_node,
                proc_buffers: ProcBuffers {
                    mono_through: MonoProcBuffersMut::new(smallvec![]),
                    indep_mono_in: MonoProcBuffers::new(smallvec![]),
                    indep_mono_out: MonoProcBuffersMut::new(smallvec![]),
                    stereo_through: StereoProcBuffersMut::new(smallvec![]),
                    indep_stereo_in: StereoProcBuffers::new(smallvec![(
                        resource_pool.get_stereo_audio_block_buffer(buffer_id),
                        0,
                    )]),
                    indep_stereo_out: StereoProcBuffersMut::new(smallvec![(delayed_buffer, 0)]),
                },
            }));

            *next_temp_stereo_block_buffer - 1
        };

    for entry in graph_schedule.iter() {
        let mut mono_in: SmallVec<
            [(
                Shared<(
                    AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                    DebugBufferID,
                )>,
                usize,
            ); SMALLVEC_ALLOC_BUFFERS],
        > = SmallVec::new();
        let mut mono_out: SmallVec<
            [(
                Shared<(
                    AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                    DebugBufferID,
                )>,
                usize,
            ); SMALLVEC_ALLOC_BUFFERS],
        > = SmallVec::new();
        let mut stereo_in: SmallVec<
            [(
                Shared<(
                    AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                    DebugBufferID,
                )>,
                usize,
            ); SMALLVEC_ALLOC_BUFFERS],
        > = SmallVec::new();
        let mut stereo_out: SmallVec<
            [(
                Shared<(
                    AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                    DebugBufferID,
                )>,
                usize,
            ); SMALLVEC_ALLOC_BUFFERS],
        > = SmallVec::new();

        // TODO: We will need to ensure that none of these buffers overlap when we start using
        // a multi-threaded schedule.
        next_temp_mono_block_buffer = 0;
        next_temp_stereo_block_buffer = 0;

        let node_id: usize = entry.node.into();

        for (port_ident, buffers) in entry.inputs.iter() {
            if buffers.len() == 1 {
                // Summing is not needed

                let (buf, delay_comp) = &buffers[0];

                let buffer_id = buf.buffer_id;
                match port_ident.port_type {
                    PortType::MonoAudio => {
                        if buffer_id > max_mono_block_buffer_id {
                            max_mono_block_buffer_id = buffer_id;
                        }

                        let buffer = if let Some(delay_comp_info) = &delay_comp {
                            // Delay compensation needed
                            let temp_buffer_id = insert_mono_delay_comp_node(
                                &mut tasks,
                                resource_pool,
                                &mut next_temp_mono_block_buffer,
                                next_delay_comp_node_id,
                                delay_comp_info,
                                entry.node,
                                *port_ident,
                                buffer_id,
                            );

                            resource_pool.get_temp_mono_audio_block_buffer(temp_buffer_id)
                        } else {
                            // No delay compensation needed
                            resource_pool.get_mono_audio_block_buffer(buffer_id)
                        };

                        mono_in.push((buffer, usize::from(port_ident.index)));
                    }
                    PortType::StereoAudio => {
                        if buffer_id > max_stereo_block_buffer_id {
                            max_stereo_block_buffer_id = buffer_id;
                        }

                        let buffer = if let Some(delay_comp_info) = &delay_comp {
                            // Delay compensation needed
                            let temp_buffer_id = insert_stereo_delay_comp_node(
                                &mut tasks,
                                resource_pool,
                                &mut next_temp_stereo_block_buffer,
                                next_delay_comp_node_id,
                                delay_comp_info,
                                entry.node,
                                *port_ident,
                                buffer_id,
                            );

                            resource_pool.get_temp_stereo_audio_block_buffer(temp_buffer_id)
                        } else {
                            // No delay compensation needed
                            resource_pool.get_stereo_audio_block_buffer(buffer_id)
                        };

                        stereo_in.push((buffer, usize::from(port_ident.index)));
                    }
                }
            } else {
                let num_inputs = buffers.len() as u32;
                match port_ident.port_type {
                    PortType::MonoAudio => {
                        let mut through_buffer = None;
                        let mut sum_indep_mono_in: SmallVec<
                            [(
                                Shared<(
                                    AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                                    DebugBufferID,
                                )>,
                                usize,
                            ); SMALLVEC_ALLOC_BUFFERS],
                        > = SmallVec::with_capacity(buffers.len());

                        for (i, (buf, delay_comp)) in buffers.iter().enumerate() {
                            let buffer_id = buf.buffer_id;
                            if buffer_id > max_mono_block_buffer_id {
                                max_mono_block_buffer_id = buffer_id;
                            }

                            let buffer = if let Some(delay_comp_info) = &delay_comp {
                                // Delay compensation needed
                                let temp_buffer_id = insert_mono_delay_comp_node(
                                    &mut tasks,
                                    resource_pool,
                                    &mut next_temp_mono_block_buffer,
                                    next_delay_comp_node_id,
                                    delay_comp_info,
                                    entry.node,
                                    *port_ident,
                                    buffer_id,
                                );

                                resource_pool.get_temp_mono_audio_block_buffer(temp_buffer_id)
                            } else {
                                // No delay compensation needed
                                resource_pool.get_mono_audio_block_buffer(buffer_id)
                            };

                            if i == 0 {
                                through_buffer = Some(buffer);
                            } else {
                                sum_indep_mono_in.push((buffer, i));
                            }
                        }

                        // This shouldn't happen.
                        if through_buffer.is_none() {
                            debug_assert!(through_buffer.is_some());
                            continue;
                        }
                        let through_buffer = through_buffer.unwrap();

                        let key = SumNodeKey {
                            node_id: node_id as u32,
                            port: *port_ident,
                        };

                        let sum_node = if let Some(old_sum_node) =
                            resource_pool.sum_nodes.get_mut(&key)
                        {
                            // Mark that this node is still being used.
                            old_sum_node.2 = true;

                            if old_sum_node.1 == num_inputs {
                                // Number of inputs has not changed, just return the existing node.
                                Shared::clone(&old_sum_node.0)
                            } else {
                                // Number of inputs has changed, replace the node.
                                let new_sum_node: Box<
                                    dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>,
                                > = Box::new(MonoSumNode::new(num_inputs));
                                let new_node = Shared::new(
                                    coll_handle,
                                    (
                                        AtomicRefCell::new(new_sum_node),
                                        DebugNodeID::Sum(*next_sum_node_id),
                                    ),
                                );
                                *next_sum_node_id += 1;

                                old_sum_node.0 = Shared::clone(&new_node);
                                old_sum_node.1 = num_inputs;

                                new_node
                            }
                        } else {
                            let new_sum_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                                Box::new(MonoSumNode::new(num_inputs));
                            let new_node = Shared::new(
                                coll_handle,
                                (
                                    AtomicRefCell::new(new_sum_node),
                                    DebugNodeID::Sum(*next_sum_node_id),
                                ),
                            );
                            *next_sum_node_id += 1;

                            let _ = resource_pool
                                .sum_nodes
                                .insert(key, (Shared::clone(&new_node), num_inputs, true));

                            new_node
                        };

                        tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                            node: sum_node,
                            proc_buffers: ProcBuffers {
                                mono_through: MonoProcBuffersMut::new(smallvec![(
                                    Shared::clone(&through_buffer),
                                    0,
                                )]),
                                indep_mono_in: MonoProcBuffers::new(sum_indep_mono_in),
                                indep_mono_out: MonoProcBuffersMut::new(smallvec![]),
                                stereo_through: StereoProcBuffersMut::new(smallvec![]),
                                indep_stereo_in: StereoProcBuffers::new(smallvec![]),
                                indep_stereo_out: StereoProcBuffersMut::new(smallvec![]),
                            },
                        }));

                        mono_in.push((through_buffer, usize::from(port_ident.index)));
                    }
                    PortType::StereoAudio => {
                        let mut through_buffer = None;
                        let mut sum_indep_stereo_in: SmallVec<
                            [(
                                Shared<(
                                    AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                                    DebugBufferID,
                                )>,
                                usize,
                            ); SMALLVEC_ALLOC_BUFFERS],
                        > = SmallVec::with_capacity(buffers.len());

                        for (i, (buf, delay_comp)) in buffers.iter().enumerate() {
                            let buffer_id = buf.buffer_id;
                            if buffer_id > max_stereo_block_buffer_id {
                                max_stereo_block_buffer_id = buffer_id;
                            }

                            let buffer = if let Some(delay_comp_info) = &delay_comp {
                                // Delay compensation needed
                                let temp_buffer_id = insert_stereo_delay_comp_node(
                                    &mut tasks,
                                    resource_pool,
                                    &mut next_temp_stereo_block_buffer,
                                    next_delay_comp_node_id,
                                    delay_comp_info,
                                    entry.node,
                                    *port_ident,
                                    buffer_id,
                                );

                                resource_pool.get_temp_stereo_audio_block_buffer(temp_buffer_id)
                            } else {
                                // No delay compensation needed
                                resource_pool.get_stereo_audio_block_buffer(buffer_id)
                            };

                            if i == 0 {
                                through_buffer = Some(buffer);
                            } else {
                                sum_indep_stereo_in.push((buffer, i));
                            }
                        }

                        // This shouldn't happen.
                        if through_buffer.is_none() {
                            debug_assert!(through_buffer.is_some());
                            continue;
                        }
                        let through_buffer = through_buffer.unwrap();

                        let key = SumNodeKey {
                            node_id: node_id as u32,
                            port: *port_ident,
                        };

                        let sum_node = if let Some(old_sum_node) =
                            resource_pool.sum_nodes.get_mut(&key)
                        {
                            // Mark that this node is still being used.
                            old_sum_node.2 = true;

                            if old_sum_node.1 == num_inputs {
                                // Number of inputs has not changed, just return the existing node.
                                Shared::clone(&old_sum_node.0)
                            } else {
                                // Number of inputs has changed, replace the node.
                                let new_sum_node: Box<
                                    dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>,
                                > = Box::new(StereoSumNode::new(num_inputs));
                                let new_node = Shared::new(
                                    coll_handle,
                                    (
                                        AtomicRefCell::new(new_sum_node),
                                        DebugNodeID::Sum(*next_sum_node_id),
                                    ),
                                );
                                *next_sum_node_id += 1;

                                old_sum_node.0 = Shared::clone(&new_node);
                                old_sum_node.1 = num_inputs;

                                new_node
                            }
                        } else {
                            let new_sum_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                                Box::new(StereoSumNode::new(num_inputs));
                            let new_node = Shared::new(
                                coll_handle,
                                (
                                    AtomicRefCell::new(new_sum_node),
                                    DebugNodeID::Sum(*next_sum_node_id),
                                ),
                            );
                            *next_sum_node_id += 1;

                            let _ = resource_pool
                                .sum_nodes
                                .insert(key, (Shared::clone(&new_node), num_inputs, true));

                            new_node
                        };

                        tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                            node: sum_node,
                            proc_buffers: ProcBuffers {
                                mono_through: MonoProcBuffersMut::new(smallvec![]),
                                indep_mono_in: MonoProcBuffers::new(smallvec![]),
                                indep_mono_out: MonoProcBuffersMut::new(smallvec![]),
                                stereo_through: StereoProcBuffersMut::new(smallvec![(
                                    Shared::clone(&through_buffer),
                                    0,
                                )]),
                                indep_stereo_in: StereoProcBuffers::new(sum_indep_stereo_in),
                                indep_stereo_out: StereoProcBuffersMut::new(smallvec![]),
                            },
                        }));

                        stereo_in.push((through_buffer, usize::from(port_ident.index)));
                    }
                }
            }
        }

        for (port_ident, buffer) in entry.outputs.iter() {
            let buffer_id = buffer.buffer_id;

            match port_ident.port_type {
                PortType::MonoAudio => {
                    if buffer_id > max_mono_block_buffer_id {
                        max_mono_block_buffer_id = buffer_id;
                    }

                    let buffer = resource_pool.get_mono_audio_block_buffer(buffer_id);

                    mono_out.push((buffer, usize::from(port_ident.index)));
                }
                PortType::StereoAudio => {
                    if buffer_id > max_stereo_block_buffer_id {
                        max_stereo_block_buffer_id = buffer_id;
                    }

                    let buffer = resource_pool.get_stereo_audio_block_buffer(buffer_id);

                    stereo_out.push((buffer, usize::from(port_ident.index)));
                }
            }
        }

        if next_temp_mono_block_buffer != 0 {
            if next_temp_mono_block_buffer - 1 > max_temp_mono_block_buffer_id {
                max_temp_mono_block_buffer_id = next_temp_mono_block_buffer - 1;
            }
        }
        if next_temp_stereo_block_buffer != 0 {
            if next_temp_stereo_block_buffer - 1 > max_temp_stereo_block_buffer_id {
                max_temp_stereo_block_buffer_id = next_temp_stereo_block_buffer - 1;
            }
        }

        let node_id: usize = entry.node.into();
        let mut found_node = None;
        if let Some(node) = resource_pool.nodes.get(node_id).as_ref() {
            if let Some(node) = node {
                found_node = Some((Shared::clone(&node.0), node.1));
            }
        }

        if root_node_scheduled {
            log::error!("Schedule error: The root node was not the last node in the schedule");
            debug_assert!(
                false,
                "Schedule error: The root node was not the last node in the schedule"
            );
        }

        if let Some((node, (mono_through_ports, stereo_through_ports))) = found_node {
            if entry.node == graph.root_node_ref {
                // In theory the root node should always be the last node in the graph, so
                // it should be safe to add an extra output buffer.
                max_stereo_block_buffer_id += 1;
                let buffer =
                    resource_pool.get_stereo_audio_block_buffer(max_stereo_block_buffer_id);

                stereo_out.push((Shared::clone(&buffer), 0));
                master_out_buffer = Some(buffer);

                root_node_scheduled = true;
            }

            let mut mono_through: SmallVec<
                [(
                    Shared<(
                        AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                        DebugBufferID,
                    )>,
                    usize,
                ); SMALLVEC_ALLOC_BUFFERS],
            > = SmallVec::new();
            let mut stereo_through: SmallVec<
                [(
                    Shared<(
                        AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                        DebugBufferID,
                    )>,
                    usize,
                ); SMALLVEC_ALLOC_BUFFERS],
            > = SmallVec::new();

            let mut copy_mono_buffers: SmallVec<
                [(
                    Shared<(
                        AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                        DebugBufferID,
                    )>,
                    Shared<(
                        AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                        DebugBufferID,
                    )>,
                ); SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();
            let mut copy_stereo_buffers: SmallVec<
                [(
                    Shared<(
                        AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                        DebugBufferID,
                    )>,
                    Shared<(
                        AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                        DebugBufferID,
                    )>,
                ); SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();

            let mut clear_mono_buffers: SmallVec<
                [Shared<(
                    AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                    DebugBufferID,
                )>; SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();
            let mut clear_stereo_buffers: SmallVec<
                [Shared<(
                    AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                    DebugBufferID,
                )>; SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();

            // TODO: Proper process_replacing() behavior instead of just copying buffers behind
            // the scenes.
            for i in 0..mono_through_ports as usize {
                let mut in_buffer_i = None;
                let mut out_buffer_i = None;
                for (buf_i, (_, port_id)) in mono_in.iter().enumerate() {
                    if *port_id == i {
                        in_buffer_i = Some(buf_i);
                        break;
                    }
                }
                for (buf_i, (_, port_id)) in mono_out.iter().enumerate() {
                    if *port_id == i {
                        out_buffer_i = Some(buf_i);
                        break;
                    }
                }

                if let Some(out_buffer_i) = out_buffer_i {
                    if let Some(in_buffer_i) = in_buffer_i {
                        let in_buffer = mono_in.remove(in_buffer_i);
                        let out_buffer = mono_out.remove(out_buffer_i);

                        copy_mono_buffers.push((in_buffer.0, Shared::clone(&out_buffer.0)));
                        mono_through.push(out_buffer);
                    } else {
                        let out_buffer = mono_out.remove(out_buffer_i);

                        clear_mono_buffers.push(Shared::clone(&out_buffer.0));
                        mono_through.push(out_buffer);
                    }
                } else if let Some(in_buffer_i) = in_buffer_i {
                    let in_buffer = mono_in.remove(in_buffer_i);

                    mono_through.push(in_buffer);
                }
            }
            for i in 0..stereo_through_ports as usize {
                let mut in_buffer_i = None;
                let mut out_buffer_i = None;
                for (buf_i, (_, port_id)) in stereo_in.iter().enumerate() {
                    if *port_id == i {
                        in_buffer_i = Some(buf_i);
                        break;
                    }
                }
                for (buf_i, (_, port_id)) in stereo_out.iter().enumerate() {
                    if *port_id == i {
                        out_buffer_i = Some(buf_i);
                        break;
                    }
                }

                if let Some(out_buffer_i) = out_buffer_i {
                    if let Some(in_buffer_i) = in_buffer_i {
                        let in_buffer = stereo_in.remove(in_buffer_i);
                        let out_buffer = stereo_out.remove(out_buffer_i);

                        copy_stereo_buffers.push((in_buffer.0, Shared::clone(&out_buffer.0)));
                        stereo_through.push(out_buffer);
                    } else {
                        let out_buffer = stereo_out.remove(out_buffer_i);

                        clear_stereo_buffers.push(Shared::clone(&out_buffer.0));
                        stereo_through.push(out_buffer);
                    }
                } else if let Some(in_buffer_i) = in_buffer_i {
                    let in_buffer = stereo_in.remove(in_buffer_i);

                    stereo_through.push(in_buffer);
                }
            }

            if !clear_mono_buffers.is_empty() {
                tasks.push(AudioGraphTask::MimicProcessReplacing(
                    MimicProcessReplacingTask::ClearMonoBuffers(clear_mono_buffers),
                ));
            }
            if !clear_stereo_buffers.is_empty() {
                tasks.push(AudioGraphTask::MimicProcessReplacing(
                    MimicProcessReplacingTask::ClearStereoBuffers(clear_stereo_buffers),
                ));
            }

            if !copy_mono_buffers.is_empty() {
                tasks.push(AudioGraphTask::MimicProcessReplacing(
                    MimicProcessReplacingTask::CopyMonoBuffers(copy_mono_buffers),
                ));
            }
            if !copy_stereo_buffers.is_empty() {
                tasks.push(AudioGraphTask::MimicProcessReplacing(
                    MimicProcessReplacingTask::CopyStereoBuffers(copy_stereo_buffers),
                ));
            }

            tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                node,
                proc_buffers: ProcBuffers {
                    mono_through: MonoProcBuffersMut::new(mono_through),
                    indep_mono_in: MonoProcBuffers::new(mono_in),
                    indep_mono_out: MonoProcBuffersMut::new(mono_out),
                    stereo_through: StereoProcBuffersMut::new(stereo_through),
                    indep_stereo_in: StereoProcBuffers::new(stereo_in),
                    indep_stereo_out: StereoProcBuffersMut::new(stereo_out),
                },
            }));
        } else {
            log::error!("Schedule error: Node with ID {} does not exist", node_id);
            debug_assert!(
                false,
                "Schedule error: Node with ID {} does not exist",
                node_id
            );
        }
    }

    let master_out_buffer = if let Some(buffer) = master_out_buffer.take() {
        buffer
    } else {
        log::error!("No master output buffer exists. This will only output silence.");
        debug_assert!(
            false,
            "No master output buffer exists. This will only output silence."
        );

        max_stereo_block_buffer_id += 1;
        resource_pool.get_temp_stereo_audio_block_buffer(max_stereo_block_buffer_id)
    };

    // Remove buffers that are no longer needed
    if resource_pool.mono_block_buffers.len() > max_mono_block_buffer_id {
        resource_pool.remove_mono_block_buffers(
            resource_pool.mono_block_buffers.len() - (max_mono_block_buffer_id + 1),
        );
    }
    if resource_pool.stereo_block_buffers.len() > max_stereo_block_buffer_id {
        resource_pool.remove_stereo_block_buffers(
            resource_pool.stereo_block_buffers.len() - (max_stereo_block_buffer_id + 1),
        );
    }
    if resource_pool.temp_mono_block_buffers.len() > max_temp_mono_block_buffer_id {
        resource_pool.remove_temp_mono_block_buffers(
            resource_pool.temp_mono_block_buffers.len() - (max_temp_mono_block_buffer_id + 1),
        );
    }
    if resource_pool.temp_stereo_block_buffers.len() > max_temp_stereo_block_buffer_id {
        resource_pool.remove_temp_stereo_block_buffers(
            resource_pool.temp_stereo_block_buffers.len() - (max_temp_stereo_block_buffer_id + 1),
        );
    }

    log::debug!(
        "Recompiled graph: | master out buffer id: {:?} | tasks: {:?}",
        master_out_buffer.1,
        &tasks
    );

    // Remove delay comp and sum nodes that are no longer needed
    resource_pool.drop_unused();

    // Create the new schedule and replace the old one

    let new_schedule = Schedule::new(tasks, graph.sample_rate, master_out_buffer);

    let new_shared_state = Shared::new(
        coll_handle,
        CompiledGraph {
            resource_pool: AtomicRefCell::new(GraphResourcePool::clone(&resource_pool)),
            schedule: AtomicRefCell::new(new_schedule),
            global_data: Shared::clone(&graph.shared_graph_state.get().global_data),
        },
    );

    // This new state will be available to the rt thread at the top of the next process loop.
    graph.shared_graph_state.set(new_shared_state);

    Ok(())
}

pub struct CompiledGraph<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    resource_pool: AtomicRefCell<GraphResourcePool<GlobalData, MAX_BLOCKSIZE>>,
    schedule: AtomicRefCell<Schedule<GlobalData, MAX_BLOCKSIZE>>,
    global_data: Shared<AtomicRefCell<GlobalData>>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    CompiledGraph<GlobalData, MAX_BLOCKSIZE>
{
    pub(crate) fn new(
        coll_handle: Handle,
        sample_rate: SampleRate,
        global_data: GlobalData,
    ) -> (
        Shared<SharedCell<CompiledGraph<GlobalData, MAX_BLOCKSIZE>>>,
        GraphResourcePool<GlobalData, MAX_BLOCKSIZE>,
    ) {
        let mut resource_pool = GraphResourcePool::new(coll_handle.clone());

        let master_out_buffer = resource_pool.get_temp_stereo_audio_block_buffer(0);

        (
            Shared::new(
                &coll_handle,
                SharedCell::new(Shared::new(
                    &coll_handle,
                    CompiledGraph {
                        resource_pool: AtomicRefCell::new(GraphResourcePool::clone(&resource_pool)),
                        schedule: AtomicRefCell::new(Schedule::new(
                            vec![],
                            sample_rate,
                            master_out_buffer,
                        )),
                        global_data: Shared::new(&coll_handle, AtomicRefCell::new(global_data)),
                    },
                )),
            ),
            resource_pool,
        )
    }

    /// Where the magic happens! Only to be used by the rt thread.
    #[cfg(not(feature = "cpal-backend"))]
    pub fn process<G: FnMut(AtomicRefMut<GlobalData>, usize)>(
        &self,
        mut out: &mut [f32],
        mut global_data_process: G,
    ) {
        // Should not panic because the non-rt thread only mutates its own clone of this resource pool. It sends
        // a clone to the rt thread via a SharedCell.
        let resource_pool = &mut *AtomicRefCell::borrow_mut(&self.resource_pool);

        // Should not panic because the non-rt thread always creates a new schedule every time before sending
        // it to the rt thread via a SharedCell.
        let schedule = &mut *AtomicRefCell::borrow_mut(&self.schedule);

        // Assume output is stereo for now.
        let mut frames_left = out.len() / 2;

        // Process in blocks.
        while frames_left > 0 {
            let frames = frames_left.min(MAX_BLOCKSIZE);

            resource_pool.clear_all_buffers(frames);

            // Process the user's global data. This should not panic because this is the only place
            // this is ever borrowed.
            {
                let global_data = AtomicRefCell::borrow_mut(&self.global_data);
                global_data_process(global_data, frames);
            }

            {
                let global_data = AtomicRefCell::borrow(&self.global_data);
                schedule.process(frames, global_data);
            }

            schedule.from_master_output_interleaved(&mut out[0..(frames * 2)]);

            out = &mut out[(frames * 2)..];
            frames_left -= frames;
        }
    }

    /// Where the magic happens! Only to be used by the rt thread.
    #[cfg(feature = "cpal-backend")]
    pub fn process<T: cpal::Sample, G: FnMut(AtomicRefMut<GlobalData>, usize)>(
        &self,
        mut out: &mut [T],
        mut global_data_process: G,
    ) {
        // Should not panic because the non-rt thread only mutates its own clone of this resource pool. It sends
        // a clone to the rt thread via a SharedCell.
        let resource_pool = &mut *AtomicRefCell::borrow_mut(&self.resource_pool);

        // Should not panic because the non-rt thread always creates a new schedule every time before sending
        // it to the rt thread via a SharedCell.
        let schedule = &mut *AtomicRefCell::borrow_mut(&self.schedule);

        // Assume output is stereo for now.
        let mut frames_left = out.len() / 2;

        // Process in blocks.
        while frames_left > 0 {
            let frames = frames_left.min(MAX_BLOCKSIZE);

            resource_pool.clear_all_buffers(frames);

            // Process the user's global data. This should not panic because this is the only place
            // this is ever borrowed.
            {
                let global_data = AtomicRefCell::borrow_mut(&self.global_data);
                global_data_process(global_data, frames);
            }

            {
                let global_data = AtomicRefCell::borrow(&self.global_data);
                schedule.process(frames, global_data);
            }

            schedule.from_master_output_interleaved(&mut out[0..(frames * 2)]);

            out = &mut out[(frames * 2)..];
            frames_left -= frames;
        }
    }
}
