use atomic_refcell::AtomicRef;
use rusty_daw_core::{ProcFrames, SampleRate, SuperFrames};

use crate::shared::SharedStereoBuffer;
use crate::task::{AudioGraphTask, MimicProcessReplacingTask};

pub struct Schedule<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    root_out: SharedStereoBuffer<f32, MAX_BLOCKSIZE>,

    tasks: Vec<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>,
    proc_info: ProcInfo<MAX_BLOCKSIZE>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    Schedule<GlobalData, MAX_BLOCKSIZE>
{
    pub(crate) fn new(
        tasks: Vec<AudioGraphTask<GlobalData, MAX_BLOCKSIZE>>,
        sample_rate: SampleRate,
        root_out: SharedStereoBuffer<f32, MAX_BLOCKSIZE>,
    ) -> Self {
        Self {
            root_out,
            tasks,
            proc_info: ProcInfo::new(sample_rate),
        }
    }

    /// Only to be used by the rt thread.
    pub fn process(
        &mut self,
        frames: ProcFrames<MAX_BLOCKSIZE>,
        global_data: AtomicRef<GlobalData>,
    ) {
        // TODO: Use multithreading for processing tasks.

        let global_data = &*global_data;

        self.proc_info.set_frames(frames);

        for task in self.tasks.iter_mut() {
            match task {
                AudioGraphTask::Node(task) => {
                    let node = &mut *task.node.borrow_mut();

                    node.process(
                        &self.proc_info,
                        task.proc_buffer_assignment.as_proc_buffers(),
                        global_data,
                    );
                }
                AudioGraphTask::MimicProcessReplacing(step) => match step {
                    MimicProcessReplacingTask::CopyMonoBuffers(task) => {
                        for (src, dst) in task.iter_mut() {
                            let src = &*src.borrow();
                            let dst = &mut *dst.borrow_mut();
                            dst.copy_frames_from(&src, frames);
                        }
                    }
                    MimicProcessReplacingTask::CopyStereoBuffers(task) => {
                        for (src, dst) in task.iter_mut() {
                            let src = &*src.borrow();
                            let dst = &mut *dst.borrow_mut();
                            dst.copy_frames_from(&src, frames);
                        }
                    }
                    MimicProcessReplacingTask::ClearMonoBuffers(task) => {
                        for buf in task.iter_mut() {
                            let buf = &mut *buf.borrow_mut();
                            buf.clear_frames(frames);
                        }
                    }
                    MimicProcessReplacingTask::ClearStereoBuffers(task) => {
                        for buf in task.iter_mut() {
                            let buf = &mut *buf.borrow_mut();
                            buf.clear_frames(frames);
                        }
                    }
                },
            }
        }
    }

    // TODO: non-stereo outputs
    /// Only to be used by the rt thread.
    #[cfg(not(feature = "cpal-backend"))]
    pub fn from_root_output_interleaved(&self, mut out: &mut [f32]) {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier. Also this is the only function that
        // ever borrows this mutably.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        let src = &*self.root_out.borrow();

        let frames = self.proc_info.frames.unchecked_frames().min(out.len() / 2);

        out = &mut out[0..frames * 2];

        for i in 0..frames {
            out[i * 2] = src.left[i];
            out[(i * 2) + 1] = src.right[i];
        }
    }

    // TODO: non-stereo outputs
    /// Only to be used by the rt thread.
    #[cfg(feature = "cpal-backend")]
    pub fn from_root_output_interleaved<T: cpal::Sample>(&self, mut out: &mut [T]) {
        // This should not panic because the schedule is always checked for data races
        // beforehand by the compiler's verifier. Also this is the only function that
        // ever borrows this mutably.
        //
        // TODO: We could probably replace this AtomicRefCell with an UnsafeCell since
        // we already checked for data races, but I'd like to keep it here for now just
        // to be extra sure that the verifier is actually working correctly. We could
        // also potentially let the user decide if they want this extra safety check
        // (at the cost of worse performance) using features.
        let src = &*self.root_out.borrow();

        let frames = self.proc_info.frames.unchecked_frames().min(out.len() / 2);

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

    /// The number of frames in this process block. This will never be larger than `MAX_BLOCKSIZE`.
    pub frames: ProcFrames<MAX_BLOCKSIZE>,

    /// The number of super-frames in this process block. A single super-frame is exactly
    /// `1 / 508,032,000` of a second. This number happens to be nicely divisible by all common sample
    /// rates, allowing changes to sample rate in a project to be a lossless process.
    ///
    /// Note that super-frames can be calculated by calling `proc_info.frames.to_super_frames(proc_info.sample_rate)`.
    /// This is just provided for convenience.
    pub super_frames: SuperFrames,
}

impl<const MAX_BLOCKSIZE: usize> ProcInfo<MAX_BLOCKSIZE> {
    fn new(sample_rate: SampleRate) -> Self {
        Self {
            sample_rate,
            sample_rate_recip: sample_rate.recip(),
            frames: ProcFrames::default(),
            super_frames: SuperFrames::default(),
        }
    }

    #[inline]
    fn set_frames(&mut self, frames: ProcFrames<MAX_BLOCKSIZE>) {
        self.frames = frames;
        self.super_frames = self.frames.to_super_frames(self.sample_rate);
    }
}
