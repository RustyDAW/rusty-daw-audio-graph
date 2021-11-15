use audio_graph::NodeRef;
use basedrop::Handle;
use fnv::FnvHashMap;
use std::cmp::Eq;
use std::hash::{Hash, Hasher};

use crate::graph_state::PortIdent;
use crate::node::AudioGraphNode;
use crate::shared::{
    SharedDelayNode, SharedMonoBuffer, SharedNode, SharedStereoBuffer, SharedSumNode,
    SharedUserNode,
};

// Total bytes = 16
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct DelayCompNodeKey {
    pub src_node_id: u32,
    pub src_node_port: PortIdent,
    pub dst_node_id: u32,
    pub dst_node_port: PortIdent,
}

// Total bytes = 8
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SumNodeKey {
    pub node_id: u32,
    pub port: PortIdent,
}

/// Used for debugging purposes.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugNodeType {
    User,
    MonoSum,
    StereoSum,
    MonoDelayComp,
    StereoDelayComp,
    Root,
}

impl std::fmt::Debug for DebugNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DebugNodeType::User => "User",
                DebugNodeType::MonoSum => "MonoSum",
                DebugNodeType::StereoSum => "StereoSum",
                DebugNodeType::MonoDelayComp => "MonoDelayComp",
                DebugNodeType::StereoDelayComp => "StereoDelayComp",
                DebugNodeType::Root => "Root",
            }
        )
    }
}

/// Used for debugging purposes.
#[derive(Clone, Copy)]
pub struct DebugNodeID {
    pub node_type: DebugNodeType,
    pub index: u64,
    pub name: Option<&'static str>,
}

impl PartialEq for DebugNodeID {
    fn eq(&self, other: &Self) -> bool {
        self.node_type.eq(&other.node_type) && self.index.eq(&other.index)
    }
}

impl Eq for DebugNodeID {}

impl Hash for DebugNodeID {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.index.hash(state);
    }
}

impl std::fmt::Debug for DebugNodeID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = self.name {
            write!(f, "User( {} ), {}", name, self.index)
        } else {
            write!(f, "{:?}, {}", self.node_type, self.index)
        }
    }
}

/// Used for debugging purposes.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum DebugBufferType {
    MonoBlock,
    StereoBlock,
    TempMonoBlock,
    TempStereoBlock,
    RootOut,
}

impl std::fmt::Debug for DebugBufferType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DebugBufferType::MonoBlock => "MB",
                DebugBufferType::StereoBlock => "SB",
                DebugBufferType::TempMonoBlock => "TM",
                DebugBufferType::TempStereoBlock => "TS",
                DebugBufferType::RootOut => "RO",
            }
        )
    }
}

/// Used for debugging purposes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DebugBufferID {
    pub buffer_type: DebugBufferType,
    pub index: u32,
}

impl std::fmt::Debug for DebugBufferID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}, {}", self.buffer_type, self.index)
    }
}

#[derive(Clone)]
pub(crate) struct GraphResourcePool<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    // We keep these pointers in a non-rt thread so we can cheaply clone and reconstruct
    // a new schedule to send to the rt thread whenever the graph is recompiled (only need
    // to copy pointers instead of whole nodes).
    pub user_nodes: Vec<Option<SharedUserNode<GlobalData, MAX_BLOCKSIZE>>>,
    pub delay_comp_nodes: FnvHashMap<DelayCompNodeKey, SharedDelayNode<GlobalData, MAX_BLOCKSIZE>>,
    pub sum_nodes: FnvHashMap<SumNodeKey, SharedSumNode<GlobalData, MAX_BLOCKSIZE>>,

    pub root_output_buffer: SharedStereoBuffer<f32, MAX_BLOCKSIZE>,

    pub mono_block_buffers: Vec<SharedMonoBuffer<f32, MAX_BLOCKSIZE>>,
    pub stereo_block_buffers: Vec<SharedStereoBuffer<f32, MAX_BLOCKSIZE>>,

    // These buffers are used as temporary input/output buffers when inserting sum and
    // delay nodes into the schedule.
    //
    // TODO: We will need to ensure that none of these buffers overlap when we start using
    // a multi-threaded schedule.
    pub temp_mono_block_buffers: Vec<SharedMonoBuffer<f32, MAX_BLOCKSIZE>>,
    pub temp_stereo_block_buffers: Vec<SharedStereoBuffer<f32, MAX_BLOCKSIZE>>,

    coll_handle: Handle,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    GraphResourcePool<GlobalData, MAX_BLOCKSIZE>
{
    /// Create a new resource pool. Only to be used by the non-rt thread.
    pub fn new(coll_handle: Handle) -> Self {
        let root_output_buffer = SharedStereoBuffer::new(
            DebugBufferID {
                buffer_type: DebugBufferType::RootOut,
                index: 0,
            },
            &coll_handle,
        );

        Self {
            user_nodes: Vec::new(),
            mono_block_buffers: Vec::new(),
            stereo_block_buffers: Vec::new(),
            temp_mono_block_buffers: Vec::new(),
            temp_stereo_block_buffers: Vec::new(),
            delay_comp_nodes: FnvHashMap::default(),
            sum_nodes: FnvHashMap::default(),
            root_output_buffer,
            coll_handle,
        }
    }

    pub fn add_user_node(
        &mut self,
        node_ref: NodeRef,
        node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>,
        debug_id: DebugNodeID,
        num_mono_replacing_ports: u32,
        num_stereo_replacing_ports: u32,
    ) {
        let index: usize = node_ref.into();
        while index >= self.user_nodes.len() {
            self.user_nodes.push(None);
        }

        self.user_nodes[index] = Some(SharedUserNode {
            node: SharedNode::new(node, debug_id, &self.coll_handle),
            num_mono_replacing_ports,
            num_stereo_replacing_ports,
        });
    }

    pub fn remove_user_node(&mut self, node_ref: NodeRef) {
        let index: usize = node_ref.into();
        self.user_nodes[index] = None;
    }

    pub fn get_mono_audio_block_buffer(
        &mut self,
        buffer_index: usize,
    ) -> SharedMonoBuffer<f32, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist
        if self.mono_block_buffers.len() <= buffer_index {
            let n_new_block_buffers = (buffer_index + 1) - self.mono_block_buffers.len();
            for _ in 0..n_new_block_buffers {
                self.mono_block_buffers.push(SharedMonoBuffer::new(
                    DebugBufferID {
                        buffer_type: DebugBufferType::MonoBlock,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.mono_block_buffers[buffer_index].clone()
    }

    pub fn get_stereo_audio_block_buffer(
        &mut self,
        buffer_index: usize,
    ) -> SharedStereoBuffer<f32, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist
        if self.stereo_block_buffers.len() <= buffer_index {
            let n_new_block_buffers = (buffer_index + 1) - self.stereo_block_buffers.len();
            for _ in 0..n_new_block_buffers {
                self.stereo_block_buffers.push(SharedStereoBuffer::new(
                    DebugBufferID {
                        buffer_type: DebugBufferType::StereoBlock,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.stereo_block_buffers[buffer_index].clone()
    }

    pub(crate) fn remove_mono_block_buffers(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.mono_block_buffers.len());
        for _ in 0..n {
            let _ = self.mono_block_buffers.pop();
        }
    }

    pub(crate) fn remove_stereo_block_buffers(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.stereo_block_buffers.len());
        for _ in 0..n {
            let _ = self.stereo_block_buffers.pop();
        }
    }

    pub fn get_temp_mono_audio_block_buffer(
        &mut self,
        buffer_index: usize,
    ) -> SharedMonoBuffer<f32, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist
        if self.temp_mono_block_buffers.len() <= buffer_index {
            let n_new_block_buffers = (buffer_index + 1) - self.temp_mono_block_buffers.len();
            for _ in 0..n_new_block_buffers {
                self.temp_mono_block_buffers.push(SharedMonoBuffer::new(
                    DebugBufferID {
                        buffer_type: DebugBufferType::TempMonoBlock,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.temp_mono_block_buffers[buffer_index].clone()
    }

    pub fn get_temp_stereo_audio_block_buffer(
        &mut self,
        buffer_index: usize,
    ) -> SharedStereoBuffer<f32, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist
        if self.temp_stereo_block_buffers.len() <= buffer_index {
            let n_new_block_buffers = (buffer_index + 1) - self.temp_stereo_block_buffers.len();
            for _ in 0..n_new_block_buffers {
                self.temp_stereo_block_buffers.push(SharedStereoBuffer::new(
                    DebugBufferID {
                        buffer_type: DebugBufferType::TempStereoBlock,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.temp_stereo_block_buffers[buffer_index].clone()
    }

    pub fn remove_temp_mono_block_buffers(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.temp_mono_block_buffers.len());
        for _ in 0..n {
            let _ = self.temp_mono_block_buffers.pop();
        }
    }

    pub fn remove_temp_stereo_block_buffers(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.temp_stereo_block_buffers.len());
        for _ in 0..n {
            let _ = self.temp_stereo_block_buffers.pop();
        }
    }

    /*
    pub fn clear_all_buffers(&mut self, frames: usize) {
        let frames = frames.min(MAX_BLOCKSIZE);

        for b in self.mono_block_buffers.iter() {
            let b = &mut *b.borrow_mut();
            b.clear_frames(frames);
        }
        for b in self.stereo_block_buffers.iter() {
            let b = &mut *b.borrow_mut();
            b.clear_frames(frames);
        }

        // The temporary buffers do not need to be cleared since they will always be filled with data
        // by the scheduler before being sent to a node.
    }
    */

    /// Flag all delay comp and sum nodes as unused.
    pub fn flag_unused(&mut self) {
        for (_, node) in self.delay_comp_nodes.iter_mut() {
            node.is_in_graph = false;
        }
        for (_, node) in self.sum_nodes.iter_mut() {
            node.is_in_graph = false;
        }
    }

    /// Drop all delay comp and sum nodes that are no longer being used.
    pub fn drop_unused(&mut self) {
        // `retain` can be quite expensive, so only use it when it is actually needed.

        let mut retain_delay_comp_nodes = false;
        for (_, node) in self.delay_comp_nodes.iter() {
            if !node.is_in_graph {
                retain_delay_comp_nodes = true;
                break;
            }
        }

        let mut retain_sum_nodes = false;
        for (_, node) in self.sum_nodes.iter() {
            if !node.is_in_graph {
                retain_sum_nodes = true;
                break;
            }
        }

        if retain_delay_comp_nodes {
            self.delay_comp_nodes.retain(|_, n| n.is_in_graph);
        }
        if retain_sum_nodes {
            self.sum_nodes.retain(|_, n| n.is_in_graph);
        }
    }
}
