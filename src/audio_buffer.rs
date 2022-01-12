use std::cell::UnsafeCell;

use basedrop::Shared;
use smallvec::SmallVec;

use clap_sys::audio_buffer::clap_audio_buffer;

// ----- SAFETY NOTE  -------------------------------------------------------
//
// We're using Shared (reference counted) pointers to buffers wrapped in
// UnsafeCell's for passing buffers around, as well as storing plain ol'
// raw pointers for passing these buffers to external plugiins. Before you
// yell at me, let me explain why:
//
// A large goal is to have first-class support for the CLAP plugin spec.
// Using raw pointers (or more specifically the types defined in the CLAP
// C bindings) will allow us to host CLAP plugins without needing to do
// any kind of conversion between specs at runtime, (just pass the raw
// pointer to the clap plugin).
//
// Also managing mutable smart pointers across multiple threads in Rust
// without using any unsafe code is just a pain.
//
// That being said, here are the reasons why safety is upheld:
//
// - Access to these UnsafeCell's and raw pointers are constrained to this
// file only. Access to buffers outside of this file are provided via a
// safe abstraction.
//   - Borrowing channels immutably and mutably through the ProcAudioBuffer
//     require borrowing `self` as immutable or mutable. Thus the rules of
//     mutable aliasing of individual channels are enforced for the user.
//   - Borrowing the raw buffers for use in external plugins is not
//     technically unsafe. Only once the user tries to dereference those
//     pointers does it count as unsafe, and Rust will still require the
//     user to use an unsafe block to do so.
// - Every time a new Schedule is compiled, it is sent to the
// Verifier to verify there exists no data races in the schedule. If a
// violation is found then the schedule is discarded and the graph is
// reverted to the previous working state. More specifically the verifer
// checks these things:
//   - Makes sure that a buffer ID does not appear twice within the same
//     ScheduledTask.
//   - Makes sure that a buffer ID does not appear in multiple parallel
//     ScheduledThreadTask's. (once we get multithreaded scheduling)
// - The lifetime of each buffer is kept track of using basedrop's Shared
// reference counting pointer type. Thus the garbage collector only
// deallocates a certain buffer once all references to that buffer are
// dropped (which only happens once the rt thread drops an old schedule
// and replaces it with a new one at the top of the process cycle).
//
// --------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct SharedAudioBuffer<
    T: Sized + Copy + Clone + Send + Default + 'static,
    const MAX_BLOCKSIZE: usize,
> {
    buffer: Shared<(UnsafeCell<[T; MAX_BLOCKSIZE]>, UniqueBufferID)>,
}

impl<T: Sized + Copy + Clone + Send + Default + 'static, const MAX_BLOCKSIZE: usize>
    SharedAudioBuffer<T, MAX_BLOCKSIZE>
{
    fn new(id: UniqueBufferID, coll_handle: &basedrop::Handle) -> Self {
        Self {
            buffer: Shared::new(
                coll_handle,
                (UnsafeCell::new([T::default(); MAX_BLOCKSIZE]), id),
            ),
        }
    }

    pub fn unique_id(&self) -> UniqueBufferID {
        self.buffer.1
    }

    #[inline]
    fn borrow(&self) -> &[T; MAX_BLOCKSIZE] {
        // Please refer to the "SAFETY NOTE" above on why this is safe.
        unsafe { &(*self.buffer.0.get()) }
    }

    #[inline]
    fn borrow_mut(&self) -> &mut [T; MAX_BLOCKSIZE] {
        // Please refer to the "SAFETY NOTE" above on why this is safe.
        unsafe { &mut (*self.buffer.0.get()) }
    }
}

pub struct ProcAudioBuffer<
    T: Sized + Copy + Clone + Send + Default + 'static,
    const MAX_BLOCKSIZE: usize,
> {
    /// The array of `raw` buffers is stored here so we can pass the
    /// appropriate pointer to external plugins.
    raw_buffers: SmallVec<[*const T; 2]>,

    channel_count: u32,
    latency: u32,     // latency from/to the audio interface
    silent_mask: u64, // mask & (1 << N) to test if channel N is silent

    /// We keep a reference-counted pointer to the same buffers in `raw` so
    /// the garbage collector can know when it is safe to deallocate unused
    /// buffers.
    rc_buffers: SmallVec<[SharedAudioBuffer<T, MAX_BLOCKSIZE>; 2]>,
}

impl<T: Sized + Copy + Clone + Send + Default + 'static, const MAX_BLOCKSIZE: usize>
    ProcAudioBuffer<T, MAX_BLOCKSIZE>
{
    pub(crate) fn new(
        buffers: SmallVec<[SharedAudioBuffer<T, MAX_BLOCKSIZE>; 2]>,
        latency: u32,
    ) -> Self {
        assert_ne!(buffers.len(), 0);

        let raw_buffers: SmallVec<[*const T; 2]> = buffers
            .iter()
            .map(|b| {
                // Please refer to the "SAFETY NOTE" above on why this is safe.
                //
                // TODO: Can we be sure that this pointer will never get moved?
                unsafe { (*b.buffer.0.get()).as_ptr() }
            })
            .collect();

        let channel_count = buffers.len() as u32;

        Self {
            raw_buffers,
            channel_count,
            latency,
            silent_mask: 0,
            rc_buffers: buffers,
        }
    }

    /// The number of channels in this buffer.
    #[inline]
    pub fn channel_count(&self) -> usize {
        self.rc_buffers.len()
    }

    /// Immutably borrow a channel.
    ///
    /// This will return `None` if the channel with the given index does not exist.
    #[inline]
    pub fn channel(&self, channel: usize) -> Option<&[T; MAX_BLOCKSIZE]> {
        self.rc_buffers.get(channel).map(|b| b.borrow())
    }

    /// Immutably borrow a channel without checking that the channel with the given index exists.
    #[inline]
    pub unsafe fn channel_unchecked(&self, channel: usize) -> &[T; MAX_BLOCKSIZE] {
        self.rc_buffers.get_unchecked(channel).borrow()
    }

    /// Mutably borrow a channel.
    ///
    /// This will return `None` if the channel with the given index does not exist.
    #[inline]
    pub fn channel_mut(&mut self, channel: usize) -> Option<&mut [T; MAX_BLOCKSIZE]> {
        self.rc_buffers.get(channel).map(|b| b.borrow_mut())
    }

    /// Mutably borrow a channel without checking that the channel with the given index exists.
    #[inline]
    pub unsafe fn channel_unchecked_mut(&mut self, channel: usize) -> &mut [T; MAX_BLOCKSIZE] {
        self.rc_buffers.get_unchecked(channel).borrow_mut()
    }

    /// Immutably borrow the first/only channel in this buffer.
    #[inline]
    pub fn mono(&self) -> &[T; MAX_BLOCKSIZE] {
        // This is safe because we assert in the constructor that the number of
        // buffers is never 0.
        unsafe { self.rc_buffers.get_unchecked(0).borrow() }
    }

    /// Mutably borrow the first/only channel in this buffer.
    #[inline]
    pub fn mono_mut(&mut self) -> &mut [T; MAX_BLOCKSIZE] {
        // This is safe because we assert in the constructor that the number of
        // buffers is never 0.
        unsafe { self.rc_buffers.get_unchecked(0).borrow_mut() }
    }

    /// Immutably borrow the first two (or only two) channels in this buffer.
    ///
    /// This will return an error if the buffer is mono.
    #[inline]
    pub fn stereo(&self) -> Option<(&[T; MAX_BLOCKSIZE], &[T; MAX_BLOCKSIZE])> {
        if self.rc_buffers.len() > 1 {
            Some((self.rc_buffers[0].borrow(), self.rc_buffers[1].borrow()))
        } else {
            None
        }
    }

    /// Immutably borrow the first two (or only two) channels in this buffer without
    /// checking if the buffer is not mono.
    #[inline]
    pub unsafe fn stereo_unchecked(&self) -> (&[T; MAX_BLOCKSIZE], &[T; MAX_BLOCKSIZE]) {
        (
            self.rc_buffers.get_unchecked(0).borrow(),
            self.rc_buffers.get_unchecked(1).borrow(),
        )
    }

    /// Mutably borrow the first two (or only two) channels in this buffer.
    ///
    /// This will return an error if the buffer is mono.
    #[inline]
    pub fn stereo_mut(&mut self) -> Option<(&mut [T; MAX_BLOCKSIZE], &mut [T; MAX_BLOCKSIZE])> {
        if self.rc_buffers.len() > 1 {
            Some((
                self.rc_buffers[0].borrow_mut(),
                self.rc_buffers[1].borrow_mut(),
            ))
        } else {
            None
        }
    }

    /// Mutably borrow the first two (or only two) channels in this buffer without
    /// checking if the buffer is not mono.
    #[inline]
    pub unsafe fn stereo_unchecked_mut(
        &mut self,
    ) -> (&mut [T; MAX_BLOCKSIZE], &mut [T; MAX_BLOCKSIZE]) {
        (
            self.rc_buffers.get_unchecked(0).borrow_mut(),
            self.rc_buffers.get_unchecked(1).borrow_mut(),
        )
    }

    // TODO: Methods for borrowing more than 2 channel buffers at a time.
}

impl<const MAX_BLOCKSIZE: usize> ProcAudioBuffer<f32, MAX_BLOCKSIZE> {
    pub fn as_clap_audio_buffer_f32(&self) -> clap_audio_buffer {
        clap_audio_buffer {
            data32: self.raw_buffers.as_ptr(),
            data64: std::ptr::null(),
            channel_count: self.channel_count,
            latency: self.latency,
            constant_mask: self.silent_mask,
        }
    }
}

impl<const MAX_BLOCKSIZE: usize> ProcAudioBuffer<f64, MAX_BLOCKSIZE> {
    pub fn as_clap_audio_buffer_f64(&self) -> clap_audio_buffer {
        clap_audio_buffer {
            data32: std::ptr::null(),
            data64: self.raw_buffers.as_ptr(),
            channel_count: self.channel_count,
            latency: self.latency,
            constant_mask: self.silent_mask,
        }
    }
}

/// Used for debugging and verifying purposes.
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum UniqueBufferType {
    /// An f32 audio buffer directly referenced by the AudioGraph
    /// compiler (by buffer index).
    AudioF32,
    /// An f64 audio buffer directly referenced by the AudioGraph
    /// compiler (by buffer index).
    AudioF64,

    /// An f32 audio buffer created for any intermediary steps.
    IntermediaryAudioF32,
    /// An f64 audio buffer created for any intermediary steps.
    IntermediaryAudioF64,
}

impl std::fmt::Debug for UniqueBufferType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                UniqueBufferType::AudioF32 => "A32",
                UniqueBufferType::AudioF64 => "A64",
                UniqueBufferType::IntermediaryAudioF32 => "IA32",
                UniqueBufferType::IntermediaryAudioF64 => "IA64",
            }
        )
    }
}

/// Used for debugging and verifying purposes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct UniqueBufferID {
    pub buffer_type: UniqueBufferType,
    pub index: u32,
}

impl std::fmt::Debug for UniqueBufferID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}_{}", self.buffer_type, self.index)
    }
}

pub(crate) struct AudioBufferPool<const MAX_BLOCKSIZE: usize> {
    audio_f32: Vec<SharedAudioBuffer<f32, MAX_BLOCKSIZE>>,
    audio_f64: Vec<SharedAudioBuffer<f64, MAX_BLOCKSIZE>>,

    intermediary_audio_f32: Vec<SharedAudioBuffer<f32, MAX_BLOCKSIZE>>,
    intermediary_audio_f64: Vec<SharedAudioBuffer<f64, MAX_BLOCKSIZE>>,

    coll_handle: basedrop::Handle,
}

impl<const MAX_BLOCKSIZE: usize> AudioBufferPool<MAX_BLOCKSIZE> {
    pub fn new(coll_handle: basedrop::Handle) -> Self {
        Self {
            audio_f32: Vec::new(),
            audio_f64: Vec::new(),

            intermediary_audio_f32: Vec::new(),
            intermediary_audio_f64: Vec::new(),

            coll_handle,
        }
    }

    /// Retrieve an f32 audio buffer directly referenced by the AudioGraph
    /// compiler (by buffer index).
    pub fn get_audio_buffer_f32(
        &mut self,
        buffer_index: usize,
    ) -> SharedAudioBuffer<f32, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist.
        if self.audio_f32.len() <= buffer_index {
            let n_new_buffers = (buffer_index + 1) - self.audio_f32.len();
            for _ in 0..n_new_buffers {
                self.audio_f32.push(SharedAudioBuffer::new(
                    UniqueBufferID {
                        buffer_type: UniqueBufferType::AudioF32,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.audio_f32[buffer_index].clone()
    }

    /// Retrieve an f64 audio buffer directly referenced by the AudioGraph
    /// compiler (by buffer index).
    pub fn get_audio_buffer_f64(
        &mut self,
        buffer_index: usize,
    ) -> SharedAudioBuffer<f64, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist.
        if self.audio_f64.len() <= buffer_index {
            let n_new_buffers = (buffer_index + 1) - self.audio_f64.len();
            for _ in 0..n_new_buffers {
                self.audio_f64.push(SharedAudioBuffer::new(
                    UniqueBufferID {
                        buffer_type: UniqueBufferType::AudioF64,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.audio_f64[buffer_index].clone()
    }

    /// Retrieve an f32 audio buffer used for any intermediary steps.
    pub fn get_intermediary_audio_buffer_f32(
        &mut self,
        buffer_index: usize,
    ) -> SharedAudioBuffer<f32, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist.
        if self.intermediary_audio_f32.len() <= buffer_index {
            let n_new_buffers = (buffer_index + 1) - self.intermediary_audio_f32.len();
            for _ in 0..n_new_buffers {
                self.intermediary_audio_f32.push(SharedAudioBuffer::new(
                    UniqueBufferID {
                        buffer_type: UniqueBufferType::IntermediaryAudioF32,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.intermediary_audio_f32[buffer_index].clone()
    }

    /// Retrieve an f64 audio buffer used for any intermediary steps.
    pub fn get_intermediary_audio_buffer_f64(
        &mut self,
        buffer_index: usize,
    ) -> SharedAudioBuffer<f64, MAX_BLOCKSIZE> {
        // Resize if buffer does not exist.
        if self.intermediary_audio_f64.len() <= buffer_index {
            let n_new_buffers = (buffer_index + 1) - self.intermediary_audio_f64.len();
            for _ in 0..n_new_buffers {
                self.intermediary_audio_f64.push(SharedAudioBuffer::new(
                    UniqueBufferID {
                        buffer_type: UniqueBufferType::IntermediaryAudioF64,
                        index: buffer_index as u32,
                    },
                    &self.coll_handle,
                ));
            }
        }

        self.intermediary_audio_f64[buffer_index].clone()
    }

    pub fn remove_audio_buffers_f32(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.audio_f32.len());
        for _ in 0..n {
            let _ = self.audio_f32.pop();
        }
    }

    pub fn remove_audio_buffers_f64(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.audio_f64.len());
        for _ in 0..n {
            let _ = self.audio_f64.pop();
        }
    }

    pub fn remove_intermediary_audio_buffers_f32(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.intermediary_audio_f32.len());
        for _ in 0..n {
            let _ = self.intermediary_audio_f32.pop();
        }
    }

    pub fn remove_intermediary_audio_buffers_f64(&mut self, n_to_remove: usize) {
        let n = n_to_remove.min(self.intermediary_audio_f64.len());
        for _ in 0..n {
            let _ = self.intermediary_audio_f64.pop();
        }
    }
}
