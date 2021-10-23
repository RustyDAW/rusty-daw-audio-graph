use std::fmt;

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use basedrop::Shared;
use num_traits::Num;
use rusty_daw_core::block_buffer::{MonoBlockBuffer, StereoBlockBuffer};

use super::{AudioGraphNode, DebugBufferID, DebugNodeID};

pub enum AudioGraphTask<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    Node(AudioGraphNodeTask<GlobalData, MAX_BLOCKSIZE>),
    CopyMonoBuffers(
        Vec<(
            Shared<(
                AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
            Shared<(
                AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
        )>,
    ),
    CopyStereoBuffers(
        Vec<(
            Shared<(
                AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
            Shared<(
                AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
        )>,
    ),
    ClearMonoBuffers(
        Vec<
            Shared<(
                AtomicRefCell<MonoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
        >,
    ),
    ClearStereoBuffers(
        Vec<
            Shared<(
                AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
        >,
    ),
}

pub struct AudioGraphNodeTask<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    pub node: Shared<(
        AtomicRefCell<Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>>,
        DebugNodeID,
    )>,
    pub proc_buffers: ProcBuffers<f32, MAX_BLOCKSIZE>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> fmt::Debug
    for AudioGraphTask<GlobalData, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AudioGraphTask::Node(task) => {
                let mut mono_through = String::new();
                let mut unpaired_mono_in = String::new();
                let mut unpaired_mono_out = String::new();
                let mut stereo_through = String::new();
                let mut unpaired_stereo_in = String::new();
                let mut unpaired_stereo_out = String::new();
                for b in task.proc_buffers.mono_through.buffers.iter() {
                    mono_through
                        .push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
                }
                for b in task.proc_buffers.unpaired_mono_in.buffers.iter() {
                    unpaired_mono_in
                        .push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
                }
                for b in task.proc_buffers.unpaired_mono_out.buffers.iter() {
                    unpaired_mono_out
                        .push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
                }
                for b in task.proc_buffers.stereo_through.buffers.iter() {
                    stereo_through
                        .push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
                }
                for b in task.proc_buffers.unpaired_stereo_in.buffers.iter() {
                    unpaired_stereo_in
                        .push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
                }
                for b in task.proc_buffers.unpaired_stereo_out.buffers.iter() {
                    unpaired_stereo_out
                        .push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
                }

                f.debug_struct("AudioGraphTask: Node")
                    .field("node_id", &format!("{:?}", task.node.1))
                    .field("mono_through", &mono_through)
                    .field("unpaired_mono_in", &unpaired_mono_in)
                    .field("unpaired_mono_out", &unpaired_mono_out)
                    .field("stereo_through", &stereo_through)
                    .field("unpaired_stereo_in", &unpaired_stereo_in)
                    .field("unpaired_stereo_out", &unpaired_stereo_out)
                    .finish()
            }
            AudioGraphTask::CopyMonoBuffers(task) => {
                let copy_mono_buffers: Vec<String> = task
                    .iter()
                    .map(|b| format!("[src buffer id: {:?}, dst buffer id: {:?}]", b.0 .1, b.1 .1))
                    .collect();
                f.debug_struct("AudioGraphTask: CopyMonoBuffers")
                    .field("copy_mono_buffers", &copy_mono_buffers)
                    .finish()
            }
            AudioGraphTask::CopyStereoBuffers(task) => {
                let copy_stereo_buffers: Vec<String> = task
                    .iter()
                    .map(|b| format!("[src buffer id: {:?}, dst buffer id: {:?}]", b.0 .1, b.1 .1))
                    .collect();
                f.debug_struct("AudioGraphTask: CopyStereoBuffers")
                    .field("copy_stereo_buffers", &copy_stereo_buffers)
                    .finish()
            }
            AudioGraphTask::ClearMonoBuffers(task) => {
                let clear_mono_buffers: Vec<String> = task
                    .iter()
                    .map(|b| format!("[buffer id: {:?}]", b.1))
                    .collect();
                f.debug_struct("AudioGraphTask: ClearMonoBuffers")
                    .field("clear_mono_buffers", &clear_mono_buffers)
                    .finish()
            }
            AudioGraphTask::ClearStereoBuffers(task) => {
                let clear_stereo_buffers: Vec<String> = task
                    .iter()
                    .map(|b| format!("[buffer id: {:?}]", b.1))
                    .collect();
                f.debug_struct("AudioGraphTask: ClearStereoBuffers")
                    .field("clear_stereo_buffers", &clear_stereo_buffers)
                    .finish()
            }
        }
    }
}

/// An abstraction that prepares buffers into a nice format for nodes.
pub struct ProcBuffers<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    /// Mono audio through port buffers assigned to this node.
    ///
    /// "Through" ports are a single pair of input/output ports that share the same buffer,
    /// equivalent to the concept of `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "through" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub mono_through: MonoProcBuffersMut<T, MAX_BLOCKSIZE>,

    /// The "unpaired" mono audio input buffers assigned to this node.
    ///
    /// "Unpaired" means that the output port is **not** a "Through" port. "Through" ports are a single
    /// pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub unpaired_mono_in: MonoProcBuffers<T, MAX_BLOCKSIZE>,

    /// The "unpaired" mono audio output buffers assigned to this node.
    ///
    /// "Unpaired" means that the output port is **not** a "Through" port. "Through" ports are a single
    /// pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub unpaired_mono_out: MonoProcBuffersMut<T, MAX_BLOCKSIZE>,

    /// Stereo audio through port buffers assigned to this node.
    ///
    /// "Through" ports are a single pair of input/output ports that share the same buffer,
    /// equivalent to the concept of `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "through" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub stereo_through: StereoProcBuffersMut<T, MAX_BLOCKSIZE>,

    /// The "unpaired" stereo audio input buffers assigned to this node.
    ///
    /// "Unpaired" means that the output port is **not** a "Through" port. "Through" ports are a single
    /// pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub unpaired_stereo_in: StereoProcBuffers<T, MAX_BLOCKSIZE>,

    /// The "unpaired" stereo audio output buffers assigned to this node.
    ///
    /// "Unpaired" means that the output port is **not** a "Through" port. "Through" ports are a single
    /// pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub unpaired_stereo_out: StereoProcBuffersMut<T, MAX_BLOCKSIZE>,
}

/// Mono audio input buffers assigned to this node.
///
/// The number of buffers may be less than the number of ports on this node. In that
/// case it just means some ports are disconnected.
pub struct MonoProcBuffers<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    buffers: Vec<(
        Shared<(
            AtomicRefCell<MonoBlockBuffer<T, MAX_BLOCKSIZE>>,
            DebugBufferID,
        )>,
        usize,
    )>,
}

impl<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> MonoProcBuffers<T, MAX_BLOCKSIZE> {
    pub(crate) fn new(
        buffers: Vec<(
            Shared<(
                AtomicRefCell<MonoBlockBuffer<T, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
            usize,
        )>,
    ) -> Self {
        Self { buffers }
    }

    /// The total number of buffers in this list.
    #[inline]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if the number of buffers in this list is `0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get a specific buffer in this list of buffers.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port. If your node cares about which specific
    /// ports are connected, please use `buffer_and_port()` instead. There
    /// will always be only one buffer per port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer(&self, index: usize) -> Option<AtomicRef<MonoBlockBuffer<T, MAX_BLOCKSIZE>>> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| AtomicRefCell::borrow(&b.0 .0))
    }

    /// Get a reference to a specific buffer in this list of buffers, while also
    /// returning the index of the port this buffer is assigned to. Use this instead of
    /// `buffer()` if your node cares about which specific ports are connected. There
    /// will always be only one buffer per port.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_and_port(
        &self,
        index: usize,
    ) -> Option<(AtomicRef<MonoBlockBuffer<T, MAX_BLOCKSIZE>>, usize)> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| (AtomicRefCell::borrow(&b.0 .0), b.1))
    }
}

/// Mono audio output buffers assigned to this node.
///
/// The number of buffers may be less than the number of ports on this node. In that
/// case it just means some ports are disconnected.
///
/// Also please note that the audio output buffers may not be cleared to 0.0. As
/// such, please do ***NOT*** read from the audio output buffers, and make sure that
/// all unused audio output buffers are manually cleared by the node. You may
/// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
pub struct MonoProcBuffersMut<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    buffers: Vec<(
        Shared<(
            AtomicRefCell<MonoBlockBuffer<T, MAX_BLOCKSIZE>>,
            DebugBufferID,
        )>,
        usize,
    )>,
}

impl<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> MonoProcBuffersMut<T, MAX_BLOCKSIZE> {
    pub(crate) fn new(
        buffers: Vec<(
            Shared<(
                AtomicRefCell<MonoBlockBuffer<T, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
            usize,
        )>,
    ) -> Self {
        Self { buffers }
    }

    /// The total number of buffers in this list.
    #[inline]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if the number of buffers in this list is `0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get a specific buffer in this list of buffers.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port. If your node cares about which specific
    /// ports are connected, please use `buffer_and_port()` instead. There
    /// will always be only one buffer per port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer(&self, index: usize) -> Option<AtomicRef<MonoBlockBuffer<T, MAX_BLOCKSIZE>>> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| AtomicRefCell::borrow(&b.0 .0))
    }

    /// Get a mutable reference to a specific buffer in this list of buffers.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port. If your node cares about which specific
    /// port are connected, please use `buffer_and_port_mut()` instead. There
    /// will always be only one buffer per port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_mut(
        &mut self,
        index: usize,
    ) -> Option<AtomicRefMut<MonoBlockBuffer<T, MAX_BLOCKSIZE>>> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| AtomicRefCell::borrow_mut(&b.0 .0))
    }

    /// Get a reference to a specific buffer in this list of buffers, while also
    /// returning the index of the port this buffer is assigned to. Use this instead of
    /// `buffer()` if your node cares about which specific ports are connected. There
    /// will always be only one buffer per port.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_and_port(
        &self,
        index: usize,
    ) -> Option<(AtomicRef<MonoBlockBuffer<T, MAX_BLOCKSIZE>>, usize)> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| (AtomicRefCell::borrow(&b.0 .0), b.1))
    }

    /// Get a mutable reference to a specific buffer in this list of buffers, while also
    /// returning the index of the port this buffer is assigned to. Use this instead of
    /// `buffer_mut()` if your node cares about which specific ports are connected. There
    /// will always be only one buffer per port.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_and_port_mut(
        &mut self,
        index: usize,
    ) -> Option<(AtomicRefMut<MonoBlockBuffer<T, MAX_BLOCKSIZE>>, usize)> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| (AtomicRefCell::borrow_mut(&b.0 .0), b.1))
    }
}

/// Stereo audio input buffers assigned to this node.
///
/// The number of buffers may be less than the number of ports on this node. In that
/// case it just means some ports are disconnected.
///
/// Also please note that the audio output buffers may not be cleared to 0.0. As
/// such, please do ***NOT*** read from the audio output buffers, and make sure that
/// all unused audio output buffers are manually cleared by the node. You may
/// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
pub struct StereoProcBuffers<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    buffers: Vec<(
        Shared<(
            AtomicRefCell<StereoBlockBuffer<T, MAX_BLOCKSIZE>>,
            DebugBufferID,
        )>,
        usize,
    )>,
}

impl<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> StereoProcBuffers<T, MAX_BLOCKSIZE> {
    pub(crate) fn new(
        buffers: Vec<(
            Shared<(
                AtomicRefCell<StereoBlockBuffer<T, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
            usize,
        )>,
    ) -> Self {
        Self { buffers }
    }

    /// The total number of buffers in this list.
    #[inline]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if the number of buffers in this list is `0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get a specific buffer in this list of buffers.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port. If your node cares about which specific
    /// ports are connected, please use `buffer_and_port()` instead. There
    /// will always be only one buffer per port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer(&self, index: usize) -> Option<AtomicRef<StereoBlockBuffer<T, MAX_BLOCKSIZE>>> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| AtomicRefCell::borrow(&b.0 .0))
    }

    /// Get a reference to a specific buffer in this list of buffers, while also
    /// returning the index of the port this buffer is assigned to. Use this instead of
    /// `buffer()` if your node cares about which specific ports are connected. There
    /// will always be only one buffer per port.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_and_port(
        &self,
        index: usize,
    ) -> Option<(AtomicRef<StereoBlockBuffer<T, MAX_BLOCKSIZE>>, usize)> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| (AtomicRefCell::borrow(&b.0 .0), b.1))
    }
}

/// Stereo audio output buffers assigned to this node.
///
/// The number of buffers may be less than the number of ports on this node. In that
/// case it just means some ports are disconnected.
///
/// Also please note that the audio output buffers may not be cleared to 0.0. As
/// such, please do ***NOT*** read from the audio output buffers, and make sure that
/// all unused audio output buffers are manually cleared by the node. You may
/// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
pub struct StereoProcBuffersMut<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    buffers: Vec<(
        Shared<(
            AtomicRefCell<StereoBlockBuffer<T, MAX_BLOCKSIZE>>,
            DebugBufferID,
        )>,
        usize,
    )>,
}

impl<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> StereoProcBuffersMut<T, MAX_BLOCKSIZE> {
    pub(crate) fn new(
        buffers: Vec<(
            Shared<(
                AtomicRefCell<StereoBlockBuffer<T, MAX_BLOCKSIZE>>,
                DebugBufferID,
            )>,
            usize,
        )>,
    ) -> Self {
        Self { buffers }
    }

    /// The total number of buffers in this list.
    #[inline]
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Returns `true` if the number of buffers in this list is `0`.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get a specific buffer in this list of buffers.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port. If your node cares about which specific
    /// ports are connected, please use `buffer_and_port()` instead. There
    /// will always be only one buffer per port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer(&self, index: usize) -> Option<AtomicRef<StereoBlockBuffer<T, MAX_BLOCKSIZE>>> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| AtomicRefCell::borrow(&b.0 .0))
    }

    /// Get a mutable reference to a specific buffer in this list of buffers.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port. If your node cares about which specific
    /// port are connected, please use `buffer_and_port_mut()` instead. There
    /// will always be only one buffer per port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_mut(
        &mut self,
        index: usize,
    ) -> Option<AtomicRefMut<StereoBlockBuffer<T, MAX_BLOCKSIZE>>> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| AtomicRefCell::borrow_mut(&b.0 .0))
    }

    /// Get a reference to a specific buffer in this list of buffers, while also
    /// returning the index of the port this buffer is assigned to. Use this instead of
    /// `buffer()` if your node cares about which specific ports are connected. There
    /// will always be only one buffer per port.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_and_port(
        &self,
        index: usize,
    ) -> Option<(AtomicRef<StereoBlockBuffer<T, MAX_BLOCKSIZE>>, usize)> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| (AtomicRefCell::borrow(&b.0 .0), b.1))
    }

    /// Get a mutable reference to a specific buffer in this list of buffers, while also
    /// returning the index of the port this buffer is assigned to. Use this instead of
    /// `buffer_mut()` if your node cares about which specific ports are connected. There
    /// will always be only one buffer per port.
    ///
    /// Note, `index` is the index that this buffer appears in the list. It is
    /// ***NOT*** the index of the port.
    ///
    /// Please note that this method borrows an atomic reference, which means that
    /// this is inefficient inside per-sample loops. Please use this method outside
    /// of loops if possible.
    ///
    /// You may safely use `unwrap()` if you have previously checked that the index
    /// is less than this struct's `len()` method. The compiler should elid the unwrap
    /// in that case.
    #[inline]
    pub fn buffer_and_port_mut(
        &mut self,
        index: usize,
    ) -> Option<(AtomicRefMut<StereoBlockBuffer<T, MAX_BLOCKSIZE>>, usize)> {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        //
        // TODO: Use unsafe instead of runtime checking? It would be more efficient,
        // but in theory a bug in the scheduler could try and assign the same buffer
        // twice in the same task or in parallel tasks, so it would be nice to
        // detect if that happens.
        self.buffers
            .get(index)
            .map(|b| (AtomicRefCell::borrow_mut(&b.0 .0), b.1))
    }
}
