use std::{cell::UnsafeCell, hash::Hash};

use basedrop::Shared;

use crate::plugin::RtProcessor;

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

/// Used for debugging and verifying purposes.
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

#[derive(Clone)]
pub(crate) struct SharedRtProcessor<const MAX_BLOCKSIZE: usize> {
    node: Shared<(
        UnsafeCell<Box<dyn RtProcessor<MAX_BLOCKSIZE>>>,
        UniqueProcessorID,
    )>,
}

impl<const MAX_BLOCKSIZE: usize> SharedRtProcessor<MAX_BLOCKSIZE> {
    fn new(
        id: UniqueProcessorID,
        coll_handle: &basedrop::Handle,
        node: Box<dyn RtProcessor<MAX_BLOCKSIZE>>,
    ) -> Self {
        Self {
            node: Shared::new(&coll_handle, (UnsafeCell::new(node), id)),
        }
    }

    pub fn unique_id(&self) -> UniqueProcessorID {
        self.node.1
    }

    #[inline]
    pub(crate) fn borrow_mut(&self) -> &mut Box<dyn RtProcessor<MAX_BLOCKSIZE>> {
        // Please refer to the "SAFETY NOTE" above on why this is safe.
        unsafe { &mut (*self.node.0.get()) }
    }
}

pub(crate) struct RtProcessorPool<const MAX_BLOCKSIZE: usize> {
    internal: Vec<SharedRtProcessor<MAX_BLOCKSIZE>>,
}
