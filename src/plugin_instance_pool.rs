use std::{cell::UnsafeCell, hash::Hash};

use basedrop::Shared;
use fnv::FnvHashMap;

use crate::{plugin::{Plugin, RtProcessor}, process::ProcessStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct UniquePluginInstanceID(u64);

impl UniquePluginInstanceID {
    pub fn new(accumulator: &mut u64) -> Self {
        *accumulator += 1;
        Self(*accumulator)
    }
}

/// Used for debugging and verifying purposes.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum UniqueProcessorType {
    Internal,
    Sum,
    DelayComp,
}

impl std::fmt::Debug for UniqueProcessorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UniqueProcessorType::Internal => "Int",
                UniqueProcessorType::Sum => "Sum",
                UniqueProcessorType::DelayComp => "Dly",
            }
        )
    }
}

#[derive(Clone, Copy)]
pub struct UniqueProcessorID {
    pub node_type: UniqueProcessorType,
    pub index: u32,

    pub name: &'static str,
}

impl std::fmt::Debug for UniqueProcessorID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.node_type {
            UniqueProcessorType::Internal => {
                write!(f, "Int({})_{}", self.name, self.index)
            }
            _ => {
                write!(f, "{:?}_{}", self.node_type, self.index)
            }
        }
    }
}

impl PartialEq for UniqueProcessorID {
    fn eq(&self, other: &Self) -> bool {
        self.node_type.eq(&other.node_type) && self.index.eq(&other.index)
    }
}

impl Hash for UniqueProcessorID {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.node_type.hash(state);
        self.index.hash(state);
    }
}

pub(crate) struct RtProcessorInstance<const MAX_BLOCKSIZE: usize> {
    pub processor: Box<dyn RtProcessor<MAX_BLOCKSIZE>>,
    pub last_process_status: ProcessStatus,
    pub active: bool,
}

#[derive(Clone)]
pub(crate) struct SharedRtProcessor<const MAX_BLOCKSIZE: usize> {
    shared: Shared<(UnsafeCell<RtProcessorInstance<MAX_BLOCKSIZE>>, UniqueProcessorID, Option<UniquePluginInstanceID>)>,
}

impl<const MAX_BLOCKSIZE: usize> SharedRtProcessor<MAX_BLOCKSIZE> {
    fn new(
        id: UniqueProcessorID,
        coll_handle: &basedrop::Handle,
        processor: Box<dyn RtProcessor<MAX_BLOCKSIZE>>,
        plugin_instance_id: Option<UniquePluginInstanceID>,
    ) -> Self {
        Self {
            shared: Shared::new(
                &coll_handle,
                (
                    UnsafeCell::new(RtProcessorInstance {
                        processor,
                        last_process_status: ProcessStatus::Continue,
                        active: true,
                    }),
                    id,
                    plugin_instance_id,
                ))
        }
    }

    pub fn unique_id(&self) -> UniqueProcessorID {
        self.shared.1
    }

    pub fn plugin_instance_id(&self) -> Option<UniquePluginInstanceID> {
        self.shared.2
    }

    #[inline]
    pub(crate) fn borrow_mut(&self) -> &mut RtProcessorInstance<MAX_BLOCKSIZE> {
        // Please refer to the "SAFETY NOTE" above on why this is safe.
        unsafe { &mut (*self.shared.0.get()) }
    }
}

pub(crate) struct PluginInstancePool<const MAX_BLOCKSIZE: usize> {
    rt_processors: FnvHashMap<UniqueProcessorID, SharedRtProcessor<MAX_BLOCKSIZE>>,
}
