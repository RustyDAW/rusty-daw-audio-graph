use atomic_refcell::{AtomicRef, AtomicRefMut};
use rusty_daw_core::block_buffer::{MonoBlockBuffer, StereoBlockBuffer};
use smallvec::SmallVec;

use crate::shared::{SharedMonoBuffer, SharedStereoBuffer};
use crate::{DebugBufferID, ProcInfo, SMALLVEC_ALLOC_PORT_BUFFERS};

pub(crate) struct ProcBufferAssignment<
    T: Default + Copy + Clone + Send + 'static,
    const MAX_BLOCKSIZE: usize,
> {
    pub mono_replacing: SmallVec<[MonoProcBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS]>,
    pub indep_mono_in: SmallVec<[MonoProcBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS]>,
    pub indep_mono_out: SmallVec<[MonoProcBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS]>,
    pub stereo_replacing:
        SmallVec<[StereoProcBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS]>,
    pub indep_stereo_in:
        SmallVec<[StereoProcBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS]>,
    pub indep_stereo_out:
        SmallVec<[StereoProcBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_PORT_BUFFERS]>,
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
    ProcBufferAssignment<T, MAX_BLOCKSIZE>
{
    #[inline]
    pub fn as_proc_buffers<'a>(&'a mut self) -> ProcBuffers<'a, T, MAX_BLOCKSIZE> {
        ProcBuffers {
            mono_replacing: self.mono_replacing.as_mut_slice(),
            indep_mono_in: self.indep_mono_in.as_slice(),
            indep_mono_out: self.indep_mono_out.as_mut_slice(),
            stereo_replacing: self.stereo_replacing.as_mut_slice(),
            indep_stereo_in: self.indep_stereo_in.as_slice(),
            indep_stereo_out: self.indep_stereo_out.as_mut_slice(),
        }
    }
}

/// The buffers assigned to a node's process method.
pub struct ProcBuffers<'a, T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize> {
    /// Mono audio "process replacing" port buffers assigned to this node.
    ///
    /// Note that the index that a buffer appears in this list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, use the `port_index()`
    /// method on the buffer struct.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case
    /// it just means some ports are disconnected.
    ///
    /// "Process replacing" ports are a single port that acts like a pair of input/output
    /// ports, but which share a single buffer. This is an optimization feature akin to
    /// "process_replacing" method in the VST spec.
    ///
    /// The scheduler will *always* use "process replacing" ports if they exist on a node,
    /// even if it has to copy input/output buffers behind the scenes. So there is no need to
    /// add a separate "non-process-replacing" version of your DSP.
    pub mono_replacing: &'a mut [MonoProcBuffer<T, MAX_BLOCKSIZE>],

    /// The "independent" mono audio input buffers assigned to this node.
    ///
    /// Note that the index that a buffer appears in this list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, use the `port_index()`
    /// method on the buffer struct.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case
    /// it just means some ports are disconnected.
    ///
    /// "Independent" means that the port is **not** a "process replacing" port. "Process
    /// replacing" ports are a single port that acts like a pair of input/output ports but which
    /// share a single buffer.
    pub indep_mono_in: &'a [MonoProcBuffer<T, MAX_BLOCKSIZE>],

    /// The "independent" mono audio output buffers assigned to this node.
    ///
    /// Note that the index that a buffer appears in this list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, use the `port_index()`
    /// method on the buffer struct.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case
    /// it just means some ports are disconnected.
    ///
    /// "Independent" means that the port is **not** a "process replacing" port. "Process
    /// replacing" ports are a single port that acts like a pair of input/output ports but which
    /// share a single buffer.
    ///
    /// Also please note that these buffers may contain junk data, so please do ***NOT*** read
    /// from these buffers before writing to them. Also, as per the spec, if you do not end up
    /// using this buffer, then it **MUST** be manually cleared to 0.0 before returning your
    /// `process` method. You may use the `clear_all_indep_out_buffers()` method for
    /// convenience.
    pub indep_mono_out: &'a mut [MonoProcBuffer<T, MAX_BLOCKSIZE>],

    /// Stereo audio "process replacing" port buffers assigned to this node.
    ///
    /// Note that the index that a buffer appears in this list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, use the `port_index()`
    /// method on the buffer struct.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case
    /// it just means some ports are disconnected.
    ///
    /// "Process replacing" ports are a single port that acts like a pair of input/output
    /// ports, but which share a single buffer. This is an optimization feature akin to
    /// "process_replacing" method in the VST spec.
    ///
    /// The scheduler will *always* use "process replacing" ports if they exist on a node,
    /// even if it has to copy input/output buffers behind the scenes. So there is no need to
    /// add a separate "non-process-replacing" version of your DSP.
    pub stereo_replacing: &'a mut [StereoProcBuffer<T, MAX_BLOCKSIZE>],

    /// The "independent" stereo audio input buffers assigned to this node.
    ///
    /// Note that the index that a buffer appears in this list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, use the `port_index()`
    /// method on the buffer struct.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case
    /// it just means some ports are disconnected.
    ///
    /// "Independent" means that the port is **not** a "process replacing" port. "Process
    /// replacing" ports are a single port that acts like a pair of input/output ports but which
    /// share a single buffer.
    pub indep_stereo_in: &'a [StereoProcBuffer<T, MAX_BLOCKSIZE>],

    /// The "independent" stereo audio output buffers assigned to this node.
    ///
    /// Note that the index that a buffer appears in this list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, use the `port_index()`
    /// method on the buffer struct.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case
    /// it just means some ports are disconnected.
    ///
    /// "Independent" means that the port is **not** a "process replacing" port. "Process
    /// replacing" ports are a single port that acts like a pair of input/output ports but which
    /// share a single buffer.
    ///
    /// Also please note that these buffers may contain junk data, so please do ***NOT*** read
    /// from these buffers before writing to them. Also, as per the spec, if you do not end up
    /// using this buffer, then it **MUST** be manually cleared to 0.0 before returning your
    /// `process` method. You may use the `clear_all_indep_out_buffers()` method for
    /// convenience.
    pub indep_stereo_out: &'a mut [StereoProcBuffer<T, MAX_BLOCKSIZE>],
}

impl<'a, T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
    ProcBuffers<'a, T, MAX_BLOCKSIZE>
{
    /// As per the spec, if you do not end up using any independent output buffers, then they
    /// **MUST** be manually cleared to 0.0 before returning your `process` method.
    ///
    /// You may use this method for convenience.
    pub fn clear_all_indep_out_buffers(&mut self, proc_info: &ProcInfo<MAX_BLOCKSIZE>) {
        let frames = proc_info.frames();

        for b in self.indep_mono_out.iter_mut() {
            let mut b = b.atomic_borrow_mut();
            b.clear_frames(frames);
        }
        for b in self.indep_stereo_out.iter_mut() {
            let mut b = b.atomic_borrow_mut();
            b.clear_frames(frames);
        }
    }
}

/// A reference to a `MonoBlockBuffer`.
///
/// Note that the index that this buffer appears in the list is ***NOT*** necessarily
/// the same as the index of the port which this buffer is assigned to. If your node
/// cares about what buffers are assigned to which ports, use the `port_index()`
/// method on this struct.
///
/// There will never be more than one buffer assigned to the same port.
pub struct MonoProcBuffer<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize> {
    pub(crate) buffer: SharedMonoBuffer<T, MAX_BLOCKSIZE>,
    port_index: usize,
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
    MonoProcBuffer<T, MAX_BLOCKSIZE>
{
    pub(crate) fn new(buffer: SharedMonoBuffer<T, MAX_BLOCKSIZE>, port_index: usize) -> Self {
        Self { buffer, port_index }
    }

    /// Get an immutable reference to the buffer.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is could be less efficient inside per-sample loops. Please use this
    /// method outside of loops if possible.
    #[inline]
    pub fn atomic_borrow(&self) -> AtomicRef<MonoBlockBuffer<T, MAX_BLOCKSIZE>> {
        self.buffer.borrow()
    }

    /// Get a mutable reference to the buffer.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is could be less efficient inside per-sample loops. Please use this
    /// method outside of loops if possible.
    #[inline]
    pub fn atomic_borrow_mut(&mut self) -> AtomicRefMut<MonoBlockBuffer<T, MAX_BLOCKSIZE>> {
        self.buffer.borrow_mut()
    }

    /// The index of this port that this buffer is assigned to.
    ///
    /// Note that the index that this buffer appears in the list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, please use this method.
    #[inline]
    pub fn port_index(&self) -> usize {
        self.port_index
    }

    #[inline]
    pub(crate) fn debug_id(&self) -> DebugBufferID {
        self.buffer.debug_id()
    }
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize> std::fmt::Debug
    for MonoProcBuffer<T, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:?}), p{}, ", self.buffer, self.port_index)
    }
}

/// A reference to a `StereoBlockBuffer`.
///
/// Note that the index that this buffer appears in the list is ***NOT*** necessarily
/// the same as the index of the port which this buffer is assigned to. If your node
/// cares about what buffers are assigned to which ports, use the `port_index()`
/// method on this struct.
///
/// There will never be more than one buffer assigned to the same port.
pub struct StereoProcBuffer<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
{
    pub(crate) buffer: SharedStereoBuffer<T, MAX_BLOCKSIZE>,
    port_index: usize,
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize>
    StereoProcBuffer<T, MAX_BLOCKSIZE>
{
    pub(crate) fn new(buffer: SharedStereoBuffer<T, MAX_BLOCKSIZE>, port_index: usize) -> Self {
        Self { buffer, port_index }
    }

    /// Get an immutable reference to the buffer.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is could be less efficient inside per-sample loops. Please use this
    /// method outside of loops if possible.
    #[inline]
    pub fn atomic_borrow(&self) -> AtomicRef<StereoBlockBuffer<T, MAX_BLOCKSIZE>> {
        self.buffer.borrow()
    }

    /// Get a mutable reference to the buffer.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is could be less efficient inside per-sample loops. Please use this
    /// method outside of loops if possible.
    #[inline]
    pub fn atomic_borrow_mut(&mut self) -> AtomicRefMut<StereoBlockBuffer<T, MAX_BLOCKSIZE>> {
        self.buffer.borrow_mut()
    }

    /// The index of this port that this buffer is assigned to.
    ///
    /// Note that the index that this buffer appears in the list is ***NOT*** necessarily
    /// the same as the index of the port which this buffer is assigned to. If your node
    /// cares about what buffers are assigned to which ports, please use this method.
    #[inline]
    pub fn port_index(&self) -> usize {
        self.port_index
    }

    #[inline]
    pub(crate) fn debug_id(&self) -> DebugBufferID {
        self.buffer.debug_id()
    }
}

impl<T: Default + Copy + Clone + Send + 'static, const MAX_BLOCKSIZE: usize> std::fmt::Debug
    for StereoProcBuffer<T, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:?}), p{}, ", self.buffer, self.port_index)
    }
}
