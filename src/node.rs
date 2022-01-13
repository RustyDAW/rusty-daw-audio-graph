use std::{cell::UnsafeCell, hash::Hash};

use basedrop::Shared;

use crate::process::{InternalAudioPorts, ProcInfo, ProcessStatus};

/// Used for debugging and verifying purposes.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum UniqueNodeType {
    Internal,
    Sum,
    DelayComp,
}

impl std::fmt::Debug for UniqueNodeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UniqueNodeType::Internal => "Int",
                UniqueNodeType::Sum => "Sum",
                UniqueNodeType::DelayComp => "Dly",
            }
        )
    }
}

/// Used for debugging and verifying purposes.
#[derive(Clone, Copy)]
pub struct UniqueNodeID {
    pub node_type: UniqueNodeType,
    pub index: u32,

    pub name: &'static str,
}

impl std::fmt::Debug for UniqueNodeID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.node_type {
            UniqueNodeType::Internal => {
                write!(f, "Int({})_{}", self.name, self.index)
            }
            _ => {
                write!(f, "{:?}_{}", self.node_type, self.index)
            }
        }
    }
}

impl PartialEq for UniqueNodeID {
    fn eq(&self, other: &Self) -> bool {
        self.node_type.eq(&other.node_type) && self.index.eq(&other.index)
    }
}

impl Hash for UniqueNodeID {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.index.hash(state);
    }
}

#[derive(Clone)]
pub(crate) struct SharedNodeRtProcessor<const MAX_BLOCKSIZE: usize> {
    node: Shared<(
        UnsafeCell<Box<dyn NodeRtProcessor<MAX_BLOCKSIZE>>>,
        UniqueNodeID,
    )>,
}

impl<const MAX_BLOCKSIZE: usize> SharedNodeRtProcessor<MAX_BLOCKSIZE> {
    fn new(
        id: UniqueNodeID,
        coll_handle: &basedrop::Handle,
        node: Box<dyn NodeRtProcessor<MAX_BLOCKSIZE>>,
    ) -> Self {
        Self {
            node: Shared::new(&coll_handle, (UnsafeCell::new(node), id)),
        }
    }

    pub fn unique_id(&self) -> UniqueNodeID {
        self.node.1
    }

    #[inline]
    pub(crate) fn borrow_mut(&self) -> &mut Box<dyn NodeRtProcessor<MAX_BLOCKSIZE>> {
        // Please refer to the "SAFETY NOTE" above on why this is safe.
        unsafe { &mut (*self.node.0.get()) }
    }
}

pub trait NodeRtProcessor<const MAX_BLOCKSIZE: usize>: Send {
    /// Called before the host starts processing/wakes the plugin from
    /// sleep.
    ///
    /// This will always be called once before the first call to `process()`.
    fn start_processing(&mut self) {}

    /// Called before the host sends the plugin to sleep.
    fn stop_processing(&mut self) {}

    /// The main process method (`f32` buffers).
    ///
    /// The host may call this method even if your plugin has set
    /// `supports_f64()` to return `true`. In that case it is up to
    /// you to decide whether to process in `f32` or convert to/from
    /// `f64` buffers manually.
    fn process(
        &mut self,
        info: &ProcInfo<MAX_BLOCKSIZE>,
        audio: &mut InternalAudioPorts<f32, MAX_BLOCKSIZE>,
    ) -> ProcessStatus;

    /// The main process method (`f64` buffers).
    ///
    /// The host will not call this method unless your plugin has
    /// set `supports_f64()` to return `true`.
    #[allow(unused)]
    fn process_f64(
        &mut self,
        info: &ProcInfo<MAX_BLOCKSIZE>,
        audio: &mut InternalAudioPorts<f64, MAX_BLOCKSIZE>,
    ) -> ProcessStatus {
        ProcessStatus::Error
    }
}

pub(crate) struct NodePool<const MAX_BLOCKSIZE: usize> {
    internal: Vec<SharedNodeRtProcessor<MAX_BLOCKSIZE>>,
}
