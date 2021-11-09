use atomic_refcell::AtomicRefCell;
use audio_graph::DelayCompInfo;
use basedrop::Shared;
use smallvec::{smallvec, SmallVec};

use crate::node::sample_delay::{MonoSampleDelayNode, StereoSampleDelayNode};
use crate::node::sum::{MonoSumNode, StereoSumNode};
use crate::proc_buffers::ProcBufferAssignment;
use crate::resource_pool::{
    DebugNodeID, DebugNodeType, DelayCompNodeKey, GraphResourcePool, SumNodeKey,
};
use crate::shared::{
    SharedDelayNode, SharedMonoBuffer, SharedNode, SharedStereoBuffer, SharedSumNode,
};
use crate::task::{AudioGraphNodeTask, AudioGraphTask, MimicProcessReplacingTask};
use crate::{
    AudioGraphExecutor, AudioGraphNode, CompilerError, CompilerWarning, GraphInterface,
    MonoProcBuffer, NodeRef, PortIdent, PortType, Schedule, StereoProcBuffer,
    SMALLVEC_ALLOC_MPR_BUFFERS, SMALLVEC_ALLOC_PORT_BUFFERS,
};

// Beware: this is one hefty boi of a function
// TODO: errors and reverting to previous working state
pub(crate) fn compile_graph<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>(
    graph: &mut GraphInterface<GlobalData, MAX_BLOCKSIZE>,
) -> Result<Option<CompilerWarning>, CompilerError> {
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

        verifier,
    } = graph;

    let mut status: Option<CompilerWarning> = None;

    let mut root_out_buffer = None;

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
                    old_delay_node.is_in_graph = true;

                    if old_delay_node.delay == new_delay {
                        // Delay has not changed, just return the existing node.
                        old_delay_node.node.clone()
                    } else {
                        // Delay has changed, replace the node.
                        let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                            Box::new(MonoSampleDelayNode::new(new_delay));
                        let new_node = SharedNode::new(
                            new_delay_node,
                            DebugNodeID {
                                node_type: DebugNodeType::MonoDelayComp,
                                index: *next_delay_comp_node_id,
                                name: None,
                            },
                            coll_handle,
                        );
                        *next_delay_comp_node_id += 1;

                        old_delay_node.node = new_node.clone();
                        old_delay_node.delay = new_delay;

                        new_node
                    }
                } else {
                    let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                        Box::new(MonoSampleDelayNode::new(new_delay));
                    let new_node = SharedNode::new(
                        new_delay_node,
                        DebugNodeID {
                            node_type: DebugNodeType::MonoDelayComp,
                            index: *next_delay_comp_node_id,
                            name: None,
                        },
                        coll_handle,
                    );
                    *next_delay_comp_node_id += 1;

                    let _ = resource_pool.delay_comp_nodes.insert(
                        key,
                        SharedDelayNode {
                            node: new_node.clone(),
                            delay: new_delay,
                            is_in_graph: true,
                        },
                    );

                    new_node
                };

            tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                node: delay_node,
                proc_buffer_assignment: ProcBufferAssignment {
                    mono_replacing: SmallVec::new(),
                    indep_mono_in: smallvec![MonoProcBuffer::new(
                        resource_pool.get_mono_audio_block_buffer(buffer_id),
                        0
                    )],
                    indep_mono_out: smallvec![MonoProcBuffer::new(delayed_buffer, 0)],
                    stereo_replacing: SmallVec::new(),
                    indep_stereo_in: SmallVec::new(),
                    indep_stereo_out: SmallVec::new(),
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
                    old_delay_node.is_in_graph = true;

                    if old_delay_node.delay == new_delay {
                        // Delay has not changed, just return the existing node.
                        old_delay_node.node.clone()
                    } else {
                        // Delay has changed, replace the node.
                        let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                            Box::new(StereoSampleDelayNode::new(new_delay));
                        let new_node = SharedNode::new(
                            new_delay_node,
                            DebugNodeID {
                                node_type: DebugNodeType::StereoDelayComp,
                                index: *next_delay_comp_node_id,
                                name: None,
                            },
                            coll_handle,
                        );
                        *next_delay_comp_node_id += 1;

                        old_delay_node.node = new_node.clone();
                        old_delay_node.delay = new_delay;

                        new_node
                    }
                } else {
                    let new_delay_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                        Box::new(StereoSampleDelayNode::new(new_delay));
                    let new_node = SharedNode::new(
                        new_delay_node,
                        DebugNodeID {
                            node_type: DebugNodeType::StereoDelayComp,
                            index: *next_delay_comp_node_id,
                            name: None,
                        },
                        coll_handle,
                    );
                    *next_delay_comp_node_id += 1;

                    let _ = resource_pool.delay_comp_nodes.insert(
                        key,
                        SharedDelayNode {
                            node: new_node.clone(),
                            delay: new_delay,
                            is_in_graph: true,
                        },
                    );

                    new_node
                };

            tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                node: delay_node,
                proc_buffer_assignment: ProcBufferAssignment {
                    mono_replacing: SmallVec::new(),
                    indep_mono_in: SmallVec::new(),
                    indep_mono_out: SmallVec::new(),
                    stereo_replacing: SmallVec::new(),
                    indep_stereo_in: smallvec![StereoProcBuffer::new(
                        resource_pool.get_stereo_audio_block_buffer(buffer_id),
                        0
                    )],
                    indep_stereo_out: smallvec![StereoProcBuffer::new(delayed_buffer, 0)],
                },
            }));

            *next_temp_stereo_block_buffer - 1
        };

    for entry in graph_schedule.iter() {
        let mut mono_in: SmallVec<
            [MonoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
        > = SmallVec::new();
        let mut mono_out: SmallVec<
            [MonoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
        > = SmallVec::new();
        let mut stereo_in: SmallVec<
            [StereoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
        > = SmallVec::new();
        let mut stereo_out: SmallVec<
            [StereoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
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

                        mono_in.push(MonoProcBuffer::new(buffer, usize::from(port_ident.index)));
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

                        stereo_in
                            .push(StereoProcBuffer::new(buffer, usize::from(port_ident.index)));
                    }
                }
            } else {
                let num_inputs = buffers.len() as u32;
                match port_ident.port_type {
                    PortType::MonoAudio => {
                        let mut through_buffer = None;
                        let mut sum_indep_mono_in: SmallVec<
                            [MonoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
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
                                sum_indep_mono_in.push(MonoProcBuffer::new(buffer, i));
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
                            old_sum_node.is_in_graph = true;

                            if old_sum_node.num_inputs == num_inputs {
                                // Number of inputs has not changed, just return the existing node.
                                old_sum_node.node.clone()
                            } else {
                                // Number of inputs has changed, replace the node.
                                let new_sum_node: Box<
                                    dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>,
                                > = Box::new(MonoSumNode::new(num_inputs));
                                let new_node = SharedNode::new(
                                    new_sum_node,
                                    DebugNodeID {
                                        node_type: DebugNodeType::MonoSum,
                                        index: *next_sum_node_id,
                                        name: None,
                                    },
                                    coll_handle,
                                );
                                *next_sum_node_id += 1;

                                old_sum_node.node = new_node.clone();
                                old_sum_node.num_inputs = num_inputs;

                                new_node
                            }
                        } else {
                            let new_sum_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                                Box::new(MonoSumNode::new(num_inputs));
                            let new_node = SharedNode::new(
                                new_sum_node,
                                DebugNodeID {
                                    node_type: DebugNodeType::MonoSum,
                                    index: *next_sum_node_id,
                                    name: None,
                                },
                                coll_handle,
                            );
                            *next_sum_node_id += 1;

                            let _ = resource_pool.sum_nodes.insert(
                                key,
                                SharedSumNode {
                                    node: new_node.clone(),
                                    num_inputs,
                                    is_in_graph: true,
                                },
                            );

                            new_node
                        };

                        tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                            node: sum_node,
                            proc_buffer_assignment: ProcBufferAssignment {
                                mono_replacing: smallvec![MonoProcBuffer::new(
                                    through_buffer.clone(),
                                    0
                                )],
                                indep_mono_in: sum_indep_mono_in,
                                indep_mono_out: SmallVec::new(),
                                stereo_replacing: SmallVec::new(),
                                indep_stereo_in: SmallVec::new(),
                                indep_stereo_out: SmallVec::new(),
                            },
                        }));

                        mono_in.push(MonoProcBuffer::new(
                            through_buffer,
                            usize::from(port_ident.index),
                        ));
                    }
                    PortType::StereoAudio => {
                        let mut through_buffer = None;
                        let mut sum_indep_stereo_in: SmallVec<
                            [StereoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
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
                                sum_indep_stereo_in.push(StereoProcBuffer::new(buffer, i));
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
                            old_sum_node.is_in_graph = true;

                            if old_sum_node.num_inputs == num_inputs {
                                // Number of inputs has not changed, just return the existing node.
                                old_sum_node.node.clone()
                            } else {
                                // Number of inputs has changed, replace the node.
                                let new_sum_node: Box<
                                    dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>,
                                > = Box::new(StereoSumNode::new(num_inputs));
                                let new_node = SharedNode::new(
                                    new_sum_node,
                                    DebugNodeID {
                                        node_type: DebugNodeType::StereoSum,
                                        index: *next_sum_node_id,
                                        name: None,
                                    },
                                    coll_handle,
                                );
                                *next_sum_node_id += 1;

                                old_sum_node.node = new_node.clone();
                                old_sum_node.num_inputs = num_inputs;

                                new_node
                            }
                        } else {
                            let new_sum_node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>> =
                                Box::new(StereoSumNode::new(num_inputs));
                            let new_node = SharedNode::new(
                                new_sum_node,
                                DebugNodeID {
                                    node_type: DebugNodeType::StereoSum,
                                    index: *next_sum_node_id,
                                    name: None,
                                },
                                coll_handle,
                            );
                            *next_sum_node_id += 1;

                            let _ = resource_pool.sum_nodes.insert(
                                key,
                                SharedSumNode {
                                    node: new_node.clone(),
                                    num_inputs,
                                    is_in_graph: true,
                                },
                            );

                            new_node
                        };

                        tasks.push(AudioGraphTask::Node(AudioGraphNodeTask {
                            node: sum_node,
                            proc_buffer_assignment: ProcBufferAssignment {
                                mono_replacing: SmallVec::new(),
                                indep_mono_in: SmallVec::new(),
                                indep_mono_out: SmallVec::new(),
                                stereo_replacing: smallvec![StereoProcBuffer::new(
                                    through_buffer.clone(),
                                    0
                                )],
                                indep_stereo_in: sum_indep_stereo_in,
                                indep_stereo_out: SmallVec::new(),
                            },
                        }));

                        stereo_in.push(StereoProcBuffer::new(
                            through_buffer,
                            usize::from(port_ident.index),
                        ));
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

                    mono_out.push(MonoProcBuffer::new(buffer, usize::from(port_ident.index)));
                }
                PortType::StereoAudio => {
                    if buffer_id > max_stereo_block_buffer_id {
                        max_stereo_block_buffer_id = buffer_id;
                    }

                    let buffer = resource_pool.get_stereo_audio_block_buffer(buffer_id);

                    stereo_out.push(StereoProcBuffer::new(buffer, usize::from(port_ident.index)));
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
        let mut found_user_node = None;
        if let Some(user_node) = resource_pool.user_nodes.get(node_id).as_ref() {
            if let Some(user_node) = user_node {
                found_user_node = Some(user_node.clone());
            }
        }

        if let Some(user_node) = found_user_node {
            if entry.node == graph.root_node_ref {
                let buffer = resource_pool.root_output_buffer.clone();

                stereo_out.push(StereoProcBuffer::new(buffer.clone(), 0));
                root_out_buffer = Some(buffer);
            }

            let mut mono_replacing: SmallVec<
                [MonoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
            > = SmallVec::new();
            let mut stereo_replacing: SmallVec<
                [StereoProcBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS],
            > = SmallVec::new();

            let mut copy_mono_buffers: SmallVec<
                [(
                    SharedMonoBuffer<f32, MAX_BLOCKSIZE>,
                    SharedMonoBuffer<f32, MAX_BLOCKSIZE>,
                ); SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();
            let mut copy_stereo_buffers: SmallVec<
                [(
                    SharedStereoBuffer<f32, MAX_BLOCKSIZE>,
                    SharedStereoBuffer<f32, MAX_BLOCKSIZE>,
                ); SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();

            let mut clear_mono_buffers: SmallVec<
                [SharedMonoBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();
            let mut clear_stereo_buffers: SmallVec<
                [SharedStereoBuffer<f32, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_MPR_BUFFERS],
            > = SmallVec::new();

            // TODO: Proper process_replacing() behavior instead of just copying buffers behind
            // the scenes.
            for i in 0..user_node.num_mono_replacing_ports as usize {
                let mut in_buffer_i = None;
                let mut out_buffer_i = None;
                for (buf_i, buf) in mono_in.iter().enumerate() {
                    if buf.port_index() == i {
                        in_buffer_i = Some(buf_i);
                        break;
                    }
                }
                for (buf_i, buf) in mono_out.iter().enumerate() {
                    if buf.port_index() == i {
                        out_buffer_i = Some(buf_i);
                        break;
                    }
                }

                if let Some(out_buffer_i) = out_buffer_i {
                    if let Some(in_buffer_i) = in_buffer_i {
                        let in_buffer = mono_in.remove(in_buffer_i);
                        let out_buffer = mono_out.remove(out_buffer_i);

                        copy_mono_buffers
                            .push((in_buffer.buffer.clone(), out_buffer.buffer.clone()));
                        mono_replacing.push(out_buffer);
                    } else {
                        let out_buffer = mono_out.remove(out_buffer_i);

                        clear_mono_buffers.push(out_buffer.buffer.clone());
                        mono_replacing.push(out_buffer);
                    }
                } else if let Some(in_buffer_i) = in_buffer_i {
                    let in_buffer = mono_in.remove(in_buffer_i);

                    mono_replacing.push(in_buffer);
                }
            }
            for i in 0..user_node.num_stereo_replacing_ports as usize {
                let mut in_buffer_i = None;
                let mut out_buffer_i = None;
                for (buf_i, buf) in stereo_in.iter().enumerate() {
                    if buf.port_index() == i {
                        in_buffer_i = Some(buf_i);
                        break;
                    }
                }
                for (buf_i, buf) in stereo_out.iter().enumerate() {
                    if buf.port_index() == i {
                        out_buffer_i = Some(buf_i);
                        break;
                    }
                }

                if let Some(out_buffer_i) = out_buffer_i {
                    if let Some(in_buffer_i) = in_buffer_i {
                        let in_buffer = stereo_in.remove(in_buffer_i);
                        let out_buffer = stereo_out.remove(out_buffer_i);

                        copy_stereo_buffers
                            .push((in_buffer.buffer.clone(), out_buffer.buffer.clone()));
                        stereo_replacing.push(out_buffer);
                    } else {
                        let out_buffer = stereo_out.remove(out_buffer_i);

                        clear_stereo_buffers.push(out_buffer.buffer.clone());
                        stereo_replacing.push(out_buffer);
                    }
                } else if let Some(in_buffer_i) = in_buffer_i {
                    let in_buffer = stereo_in.remove(in_buffer_i);

                    stereo_replacing.push(in_buffer);
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
                node: user_node.node.clone(),
                proc_buffer_assignment: ProcBufferAssignment {
                    mono_replacing,
                    indep_mono_in: mono_in,
                    indep_mono_out: mono_out,
                    stereo_replacing,
                    indep_stereo_in: stereo_in,
                    indep_stereo_out: stereo_out,
                },
            }));
        } else {
            log::error!("Compiler error: Node with index {} does not exist", node_id);
            log::error!("State of failed schedule: | tasks: {:?}", &tasks);

            return Err(CompilerError::UserNodeDoesNotExist(node_id));
        }
    }

    let root_out_buffer = if let Some(buffer) = root_out_buffer.take() {
        buffer
    } else {
        log::trace!("No master output buffer exists. This will only output silence.");
        status = Some(CompilerWarning::NothingConnectedToRoot);

        max_stereo_block_buffer_id += 1;
        resource_pool.get_stereo_audio_block_buffer(max_stereo_block_buffer_id)
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

    if let Err(e) = verifier.verify_no_data_races(&tasks) {
        log::error!("{}", &e);
        log::error!(
            "State of failed schedule: | root out buffer id: {:?} | tasks: {:?}",
            root_out_buffer,
            &tasks
        );

        return Err(e);
    }

    log::debug!("Successfully compiled schedule");
    log::trace!(
        "State of new schedule: | root out buffer id: {:?} | tasks: {:?}",
        root_out_buffer,
        &tasks
    );

    // Remove delay comp and sum nodes that are no longer needed
    resource_pool.drop_unused();

    // Create the new schedule and replace the old one

    let new_schedule = Schedule::new(tasks, graph.sample_rate, root_out_buffer);

    let new_shared_state = Shared::new(
        coll_handle,
        AudioGraphExecutor {
            schedule: AtomicRefCell::new(new_schedule),
            global_data: Shared::clone(&graph.shared_graph_state.get().global_data),
        },
    );

    // This new state will be available to the rt thread at the top of the next process loop.
    graph.shared_graph_state.set(new_shared_state);

    Ok(status)
}
