use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use basedrop::{Handle, Shared};
use rusty_daw_core::block_buffer::{MonoBlockBuffer, StereoBlockBuffer};

use crate::node::AudioGraphNode;
use crate::resource_pool::{DebugBufferID, DebugNodeID};

pub(crate) struct SharedNode<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    node: Shared<(
        AtomicRefCell<Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>>,
        DebugNodeID,
    )>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    SharedNode<GlobalData, MAX_BLOCKSIZE>
{
    pub fn new(
        node: Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>,
        debug_id: DebugNodeID,
        coll_handle: &Handle,
    ) -> Self {
        Self {
            node: Shared::new(coll_handle, (AtomicRefCell::new(node), debug_id)),
        }
    }

    #[inline]
    pub fn borrow_mut(&self) -> AtomicRefMut<Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>> {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier. The only place this method is ever
        // called is in `Schedule::process()`.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        AtomicRefCell::borrow_mut(&self.node.0)
    }

    #[inline]
    pub fn debug_id(&self) -> DebugNodeID {
        self.node.1
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> Clone
    for SharedNode<GlobalData, MAX_BLOCKSIZE>
{
    fn clone(&self) -> Self {
        Self {
            node: Shared::clone(&self.node),
        }
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> std::fmt::Debug
    for SharedNode<GlobalData, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.node.1.fmt(f)
    }
}

#[derive(Clone)]
pub(crate) struct SharedUserNode<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    pub node: SharedNode<GlobalData, MAX_BLOCKSIZE>,
    pub num_mono_replacing_ports: u32,
    pub num_stereo_replacing_ports: u32,
}

#[derive(Clone)]
pub(crate) struct SharedDelayNode<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    pub node: SharedNode<GlobalData, MAX_BLOCKSIZE>,
    pub delay: u32,
    pub is_in_graph: bool,
}

#[derive(Clone)]
pub(crate) struct SharedSumNode<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    pub node: SharedNode<GlobalData, MAX_BLOCKSIZE>,
    pub num_inputs: u32,
    pub is_in_graph: bool,
}

pub(crate) struct SharedMonoBuffer<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    buffer: Shared<(
        AtomicRefCell<MonoBlockBuffer<T, MAX_BLOCKSIZE>>,
        DebugBufferID,
    )>,
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
    SharedMonoBuffer<T, MAX_BLOCKSIZE>
{
    pub fn new(debug_id: DebugBufferID, coll_handle: &Handle) -> Self {
        Self {
            buffer: Shared::new(
                coll_handle,
                (AtomicRefCell::new(MonoBlockBuffer::new()), debug_id),
            ),
        }
    }

    #[inline]
    pub fn borrow(&self) -> AtomicRef<MonoBlockBuffer<T, MAX_BLOCKSIZE>> {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier. The only place this method is ever
        // called is in `Schedule::process()`.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        AtomicRefCell::borrow(&self.buffer.0)
    }

    #[inline]
    pub fn borrow_mut(&mut self) -> AtomicRefMut<MonoBlockBuffer<T, MAX_BLOCKSIZE>> {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier. The only place this method is ever
        // called is in `Schedule::process()`.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        AtomicRefCell::borrow_mut(&self.buffer.0)
    }

    #[inline]
    pub fn debug_id(&self) -> DebugBufferID {
        self.buffer.1
    }
}

impl<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> Clone
    for SharedMonoBuffer<T, MAX_BLOCKSIZE>
{
    fn clone(&self) -> Self {
        Self {
            buffer: Shared::clone(&self.buffer),
        }
    }
}

impl<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> std::fmt::Debug
    for SharedMonoBuffer<T, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.buffer.1.fmt(f)
    }
}

pub(crate) struct SharedStereoBuffer<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    buffer: Shared<(
        AtomicRefCell<StereoBlockBuffer<T, MAX_BLOCKSIZE>>,
        DebugBufferID,
    )>,
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
    SharedStereoBuffer<T, MAX_BLOCKSIZE>
{
    pub fn new(debug_id: DebugBufferID, coll_handle: &Handle) -> Self {
        Self {
            buffer: Shared::new(
                coll_handle,
                (AtomicRefCell::new(StereoBlockBuffer::new()), debug_id),
            ),
        }
    }

    #[inline]
    pub fn borrow(&self) -> AtomicRef<StereoBlockBuffer<T, MAX_BLOCKSIZE>> {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        AtomicRefCell::borrow(&self.buffer.0)
    }

    #[inline]
    pub fn borrow_mut(&mut self) -> AtomicRefMut<StereoBlockBuffer<T, MAX_BLOCKSIZE>> {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        AtomicRefCell::borrow_mut(&self.buffer.0)
    }

    #[inline]
    pub fn debug_id(&self) -> DebugBufferID {
        self.buffer.1
    }
}

impl<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> Clone
    for SharedStereoBuffer<T, MAX_BLOCKSIZE>
{
    fn clone(&self) -> Self {
        Self {
            buffer: Shared::clone(&self.buffer),
        }
    }
}

impl<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> std::fmt::Debug
    for SharedStereoBuffer<T, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.buffer.1.fmt(f)
    }
}
