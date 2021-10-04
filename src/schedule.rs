use atomic_refcell::{AtomicRef, AtomicRefCell};
use basedrop::Shared;
use rusty_daw_core::block_buffer::StereoBlockBuffer;
use rusty_daw_core::SampleRate;

use super::{AudioGraphTask, DebugBufferID};

pub struct Schedule<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    master_out: Shared<(
        AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
        DebugBufferID,
    )>,

    tasks: Vec<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>,
    proc_info: ProcInfo<MAX_BLOCKSIZE>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    Schedule<GlobalData, MAX_BLOCKSIZE>
{
    pub fn new(
        tasks: Vec<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>,
        sample_rate: SampleRate,
        master_out: Shared<(
            AtomicRefCell<StereoBlockBuffer<f32, MAX_BLOCKSIZE>>,
            DebugBufferID,
        )>,
    ) -> Self {
        Self {
            master_out,
            tasks,
            proc_info: ProcInfo::new(sample_rate),
        }
    }

    /// Only to be used by the rt thread.
    pub fn process(&mut self, frames: usize, global_data: AtomicRef<GlobalData>) {
        // TODO: Use multithreading for processing tasks.

        let global_data = &*global_data;

        self.proc_info.set_frames(frames);

        // Where the magic happens!
        for task in self.tasks.iter_mut() {
            // This should not panic because the rt thread is the only place these nodes
            // are borrowed.
            //
            // TODO: Use unsafe instead of runtime checking? It would be more efficient,
            // but in theory a bug in the scheduler could try and assign the same node
            // twice in parallel tasks, so it would be nice to detect if that happens.
            let node = &mut *AtomicRefCell::borrow_mut(&task.node.0);

            node.process(&self.proc_info, &mut task.proc_buffers, global_data);
        }
    }

    // TODO: non-stereo outputs
    /// Only to be used by the rt thread.
    #[cfg(not(feature = "cpal-backend"))]
    pub fn from_master_output_interleaved(&self, mut out: &mut [f32]) {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        let src = &mut *AtomicRefCell::borrow_mut(&self.master_out.0);

        let frames = self.proc_info.frames.min(out.len() / 2);

        out = &mut out[0..frames * 2];

        for i in 0..frames {
            out[i * 2] = src.left[i];
            out[(i * 2) + 1] = src.right[i];
        }
    }

    // TODO: non-stereo outputs
    /// Only to be used by the rt thread.
    #[cfg(feature = "cpal-backend")]
    pub fn from_master_output_interleaved<T: cpal::Sample>(&self, mut out: &mut [T]) {
        // This should not panic because the rt thread is the only place these buffers
        // are borrowed.
        let src = &mut *AtomicRefCell::borrow_mut(&self.master_out.0);

        let frames = self.proc_info.frames.min(out.len() / 2);

        out = &mut out[0..frames * 2];

        for i in 0..frames {
            out[i * 2] = T::from(&src.left[i]);
            out[(i * 2) + 1] = T::from(&src.right[i]);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProcInfo<const MAX_BLOCKSIZE: usize> {
    /// The sample rate of the stream. This remains constant for the whole lifetime of this node,
    /// so this is just provided for convenience.
    pub sample_rate: SampleRate,

    /// The recipricol of the sample rate (1.0 / sample_rate) of the stream. This remains constant
    /// for the whole lifetime of this node, so this is just provided for convenience.
    pub sample_rate_recip: f64,

    frames: usize,
}

impl<const MAX_BLOCKSIZE: usize> ProcInfo<MAX_BLOCKSIZE> {
    fn new(sample_rate: SampleRate) -> Self {
        Self {
            sample_rate,
            sample_rate_recip: sample_rate.recip(),
            frames: 0,
        }
    }

    #[inline]
    fn set_frames(&mut self, frames: usize) {
        self.frames = frames.min(MAX_BLOCKSIZE);
    }

    /// The number of audio frames in this current process block.
    ///
    /// This will always be less than or equal to `MAX_BLOCKSIZE`.
    ///
    /// Note, for optimization purposes, this internally looks like
    /// `self.frames.min(MAX_BLOCKSIZE)`. This allows the compiler to
    /// safely optimize loops over buffers with length `MAX_BLOCKSIZE`
    /// by eliding all bounds checking and allowing for more aggressive
    /// auto-vectorization optimizations. If you need to use this multiple
    /// times within the same function, please only call this once and store
    /// it in a local variable to avoid running this internal check every
    /// subsequent time.
    #[inline]
    pub fn frames(&self) -> usize {
        self.frames.min(MAX_BLOCKSIZE)
    }
}
