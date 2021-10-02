use std::fmt;

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use basedrop::Shared;
use num_traits::Num;
use rusty_daw_core::block_buffer::{MonoBlockBuffer, StereoBlockBuffer};

use super::{AudioGraphNode, DebugBufferID, DebugNodeID, ProcInfo};

pub struct AudioGraphTask<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
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
        let mut mono_audio_in = String::new();
        let mut mono_audio_out = String::new();
        let mut stereo_audio_in = String::new();
        let mut stereo_audio_out = String::new();

        for b in self.proc_buffers.mono_audio_in.buffers.iter() {
            mono_audio_in.push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
        }
        for b in self.proc_buffers.mono_audio_out.buffers.iter() {
            mono_audio_out.push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
        }
        for b in self.proc_buffers.stereo_audio_in.buffers.iter() {
            stereo_audio_in.push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
        }
        for b in self.proc_buffers.stereo_audio_out.buffers.iter() {
            stereo_audio_out.push_str(&format!("[buffer id: {:?}, port index: {}], ", b.0 .1, b.1));
        }

        f.debug_struct("AudioGraphTask")
            .field("node_id", &format!("{:?}", self.node.1))
            .field("mono_audio_in", &mono_audio_in)
            .field("mono_audio_out", &mono_audio_out)
            .field("stereo_audio_in", &stereo_audio_in)
            .field("stereo_audio_out", &stereo_audio_out)
            .finish()
    }
}

/// An abstraction that prepares buffers into a nice format for nodes.
pub struct ProcBuffers<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    /// Mono audio input buffers assigned to this node.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub mono_audio_in: MonoProcBuffers<T, MAX_BLOCKSIZE>,

    /// Mono audio output buffers assigned to this node.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
    pub mono_audio_out: MonoProcBuffersMut<T, MAX_BLOCKSIZE>,

    /// Stereo audio input buffers assigned to this node.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    pub stereo_audio_in: StereoProcBuffers<T, MAX_BLOCKSIZE>,

    /// Stereo audio output buffers assigned to this node.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that
    /// case it just means some ports are disconnected.
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
    pub stereo_audio_out: StereoProcBuffersMut<T, MAX_BLOCKSIZE>,
}

impl<T: Num + Copy + Clone, const MAX_BLOCKSIZE: usize> ProcBuffers<T, MAX_BLOCKSIZE> {
    /// Clears all output buffers to 0.0.
    ///
    /// This exists because audio output buffers may not be cleared to 0.0 before being sent
    /// to a node. As such, this spec requires all unused audio output buffers to be manually
    /// cleared by the node itself. This is provided as a convenience method.
    pub fn clear_audio_out_buffers(&mut self, proc_info: &ProcInfo<MAX_BLOCKSIZE>) {
        let frames = proc_info.frames();

        for b in self.mono_audio_out.buffers.iter() {
            // This should not panic because the rt thread is the only place these buffers
            // are borrowed.
            //
            // TODO: Use unsafe instead of runtime checking? It would be more efficient,
            // but in theory a bug in the scheduler could try and assign the same buffer
            // twice in the same task or in parallel tasks, so it would be nice to
            // detect if that happens.
            (&mut *AtomicRefCell::borrow_mut(&b.0 .0)).clear_frames(frames);
        }

        for b in self.stereo_audio_out.buffers.iter() {
            // This should not panic because the rt thread is the only place these buffers
            // are borrowed.
            //
            // TODO: Use unsafe instead of runtime checking? It would be more efficient,
            // but in theory a bug in the scheduler could try and assign the same buffer
            // twice in the same task or in parallel tasks, so it would be nice to
            // detect if that happens.
            (&mut *AtomicRefCell::borrow_mut(&b.0 .0)).clear_frames(frames);
        }
    }
}

/// Mono audio input buffers assigned to this node.
///
/// The number of buffers may be less than the number of ports on this node. In that
/// case it just means some ports are disconnected.
///
/// Also please note that the audio output buffers may not be cleared to 0.0. As
/// such, please do ***NOT*** read from the audio output buffers, and make sure that
/// all unused audio output buffers are manually cleared by the node. You may
/// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
    ///
    /// Also please note that the audio output buffers may not be cleared to 0.0. As
    /// such, please do ***NOT*** read from the audio output buffers, and make sure that
    /// all unused audio output buffers are manually cleared by the node. You may
    /// use `ProcBuffers::clear_audio_out_buffers()` for convenience.
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
