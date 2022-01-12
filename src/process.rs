use clap_sys::process::{
    clap_process, clap_process_status, CLAP_PROCESS_CONTINUE, CLAP_PROCESS_CONTINUE_IF_NOT_QUIET,
    CLAP_PROCESS_ERROR, CLAP_PROCESS_SLEEP,
};
use rusty_daw_core::{Frames, ProcFrames};

use crate::audio_buffer::ProcAudioBuffer;

/// The status of a call to a plugin's `process()` method.
#[non_exhaustive]
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum ProcessStatus {
    /// Processing failed. The output buffer must be discarded.
    Error = CLAP_PROCESS_ERROR,

    /// Processing succeeded, keep processing.
    Continue = CLAP_PROCESS_CONTINUE,

    /// Processing succeeded, keep processing if the output is not quiet.
    ContinueIfNotQuiet = CLAP_PROCESS_CONTINUE_IF_NOT_QUIET,

    /// Processing succeeded, but no more processing is required until
    /// the next event or variation in audio input.
    Sleep = CLAP_PROCESS_SLEEP,
}

impl ProcessStatus {
    pub fn from_clap(status: clap_process_status) -> Option<ProcessStatus> {
        match status {
            CLAP_PROCESS_ERROR => Some(ProcessStatus::Error),
            CLAP_PROCESS_CONTINUE => Some(ProcessStatus::Error),
            CLAP_PROCESS_CONTINUE_IF_NOT_QUIET => Some(ProcessStatus::Error),
            CLAP_PROCESS_SLEEP => Some(ProcessStatus::Error),
            _ => None,
        }
    }

    pub fn to_clap(&self) -> clap_process_status {
        match self {
            ProcessStatus::Error => CLAP_PROCESS_ERROR,
            ProcessStatus::Continue => CLAP_PROCESS_CONTINUE,
            ProcessStatus::ContinueIfNotQuiet => CLAP_PROCESS_CONTINUE_IF_NOT_QUIET,
            ProcessStatus::Sleep => CLAP_PROCESS_SLEEP,
        }
    }
}

pub struct ProcInfo<const MAX_BLOCKSIZE: usize> {
    raw: clap_process,

    steady_time: Option<Frames>,
    frames: ProcFrames<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> ProcInfo<MAX_BLOCKSIZE> {
    pub(crate) fn from_clap(info: clap_process) -> Self {
        let steady_time = if info.steady_time < 0 {
            None
        } else {
            Some(Frames::new(info.steady_time as u64))
        };
        let frames: ProcFrames<MAX_BLOCKSIZE> = ProcFrames::new(info.frames_count as usize);

        Self {
            raw: info,

            steady_time,
            frames,
        }
    }

    pub(crate) fn as_clap(&self) -> &clap_process {
        &self.raw
    }

    /// A steady sample time counter.
    ///
    /// This field can be used to calculate the sleep duration between two process calls.
    /// This value may be specific to this plugin instance and have no relation to what
    /// other plugin instances may receive.
    ///
    /// This will return `None` if not available, otherwise the value will be increased by
    /// at least `frames_count` for the next call to process.
    #[inline]
    pub fn steady_time(&self) -> Option<Frames> {
        self.steady_time
    }

    /// The number of frames to process.
    #[inline]
    pub fn frames(&self) -> ProcFrames<MAX_BLOCKSIZE> {
        self.frames
    }
}

pub struct ProcAudioPorts<
    T: Sized + Copy + Clone + Send + Default + 'static,
    const MAX_BLOCKSIZE: usize,
> {
    /// The main audio input buffer.
    ///
    /// Note this may be `None` even when a main input port exists.
    /// In that case it means the host has given the same buffer for
    /// the main input and output ports (process replacing).
    pub main_in: Option<ProcAudioBuffer<T, MAX_BLOCKSIZE>>,

    /// The main audio output buffer.
    pub main_out: Option<ProcAudioBuffer<T, MAX_BLOCKSIZE>>,

    /// The extra inputs buffers (not including the main input buffer).
    pub extra_inputs: Vec<ProcAudioBuffer<T, MAX_BLOCKSIZE>>,

    /// The extra output buffers (not including the main input buffer).
    pub extra_outputs: Vec<ProcAudioBuffer<T, MAX_BLOCKSIZE>>,
}

pub enum MonoInOutStatus<
    'a,
    T: Sized + Copy + Clone + Send + Default + 'static,
    const MAX_BLOCKSIZE: usize,
> {
    /// The host has given a single buffer for both the input and output port.
    InPlace(&'a mut [T; MAX_BLOCKSIZE]),

    /// The host has given a separate input and output buffer.
    Separate {
        input: &'a [T; MAX_BLOCKSIZE],
        output: &'a mut [T; MAX_BLOCKSIZE],
    },

    /// The host has not given a main mono output buffer.
    NoMonoOut,
}

pub enum StereoInOutStatus<
    'a,
    T: Sized + Copy + Clone + Send + Default + 'static,
    const MAX_BLOCKSIZE: usize,
> {
    /// The host has given a single buffer for both the input and output port.
    InPlace((&'a mut [T; MAX_BLOCKSIZE], &'a mut [T; MAX_BLOCKSIZE])),

    /// The host has given a separate input and output buffer.
    Separate {
        input: (&'a [T; MAX_BLOCKSIZE], &'a [T; MAX_BLOCKSIZE]),
        output: (&'a mut [T; MAX_BLOCKSIZE], &'a mut [T; MAX_BLOCKSIZE]),
    },

    /// The host has not given a stereo output buffer.
    NoStereoOut,
}

impl<T: Sized + Copy + Clone + Send + Default + 'static, const MAX_BLOCKSIZE: usize>
    ProcAudioPorts<T, MAX_BLOCKSIZE>
{
    /// A helper method to retrieve the main mono input/output buffers.
    pub fn main_mono_in_out<'a>(&'a mut self) -> MonoInOutStatus<'a, T, MAX_BLOCKSIZE> {
        let Self {
            main_in, main_out, ..
        } = self;

        if let Some(main_out) = main_out {
            if let Some(main_in) = main_in {
                return MonoInOutStatus::Separate {
                    input: main_in.mono(),
                    output: main_out.mono_mut(),
                };
            } else {
                return MonoInOutStatus::InPlace(main_out.mono_mut());
            }
        }

        MonoInOutStatus::NoMonoOut
    }

    /// A helper method to retrieve the main stereo input/output buffers.
    pub fn main_stereo_in_out<'a>(&'a mut self) -> StereoInOutStatus<'a, T, MAX_BLOCKSIZE> {
        let Self {
            main_in, main_out, ..
        } = self;

        if let Some(main_out) = main_out {
            if let Some(out_bufs) = main_out.stereo_mut() {
                if let Some(main_in) = main_in {
                    if let Some(in_bufs) = main_in.stereo() {
                        return StereoInOutStatus::Separate {
                            input: in_bufs,
                            output: out_bufs,
                        };
                    } else {
                        return StereoInOutStatus::InPlace(out_bufs);
                    }
                } else {
                    return StereoInOutStatus::InPlace(out_bufs);
                }
            }
        }

        StereoInOutStatus::NoStereoOut
    }

    /// A helper method to retrieve the main mono output buffer.
    #[inline]
    pub fn main_mono_out<'a>(&'a mut self) -> Option<&'a mut [T; MAX_BLOCKSIZE]> {
        if let Some(main_out) = &mut self.main_out {
            Some(main_out.mono_mut())
        } else {
            None
        }
    }

    /// A helper method to retrieve the main stereo output buffer.
    #[inline]
    pub fn main_stereo_out<'a>(
        &'a mut self,
    ) -> Option<(&'a mut [T; MAX_BLOCKSIZE], &'a mut [T; MAX_BLOCKSIZE])> {
        if let Some(main_out) = &mut self.main_out {
            if let Some(out_bufs) = main_out.stereo_mut() {
                return Some(out_bufs);
            }
        }

        None
    }
}

pub trait Processor<const MAX_BLOCKSIZE: usize> {}
