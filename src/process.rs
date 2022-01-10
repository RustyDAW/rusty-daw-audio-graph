use clap_sys::process::{
    clap_process, clap_process_status, CLAP_PROCESS_CONTINUE, CLAP_PROCESS_CONTINUE_IF_NOT_QUIET,
    CLAP_PROCESS_ERROR, CLAP_PROCESS_SLEEP,
};
use rusty_daw_core::{Frames, ProcFrames};

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
