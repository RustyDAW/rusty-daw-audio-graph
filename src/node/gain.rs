use rusty_daw_core::{ParamF32, ParamF32UiHandle, SampleRate, Unit};

use super::{DB_GRADIENT, SMOOTH_SECS};
use crate::{AudioGraphNode, ProcBuffers, ProcInfo};

pub struct GainNodeUiHandle {
    pub gain_db: ParamF32UiHandle,
}

pub struct MonoGainNode<const MAX_BLOCKSIZE: usize> {
    gain_amp: ParamF32<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> MonoGainNode<MAX_BLOCKSIZE> {
    pub fn new(
        gain_db: f32,
        min_db: f32,
        max_db: f32,
        sample_rate: SampleRate,
    ) -> (Self, GainNodeUiHandle) {
        let (gain_amp, gain_handle) = ParamF32::from_value(
            gain_db,
            min_db,
            max_db,
            DB_GRADIENT,
            Unit::Decibels,
            SMOOTH_SECS,
            sample_rate,
        );

        (
            Self { gain_amp },
            GainNodeUiHandle {
                gain_db: gain_handle,
            },
        )
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for MonoGainNode<MAX_BLOCKSIZE>
{
    fn debug_name(&self) -> &'static str {
        "RustyDAWAudioGraph::MonoGain"
    }

    fn mono_replacing_ports(&self) -> u32 {
        1
    }

    fn process(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: ProcBuffers<f32, MAX_BLOCKSIZE>,
        _global_data: &GlobalData,
    ) {
        if buffers.mono_replacing.is_empty() {
            return;
        }

        let buf = &mut *buffers.mono_replacing[0].atomic_borrow_mut();
        let gain_amp = self.gain_amp.smoothed(proc_info.frames);
        let frames = proc_info.frames.compiler_hint_frames();

        // TODO: SIMD

        if gain_amp.is_smoothing() {
            for i in 0..frames {
                buf.buf[i] *= gain_amp[i];
            }
        } else {
            // We can optimize by using a constant gain (better SIMD load efficiency).
            let gain = gain_amp[0];

            if !(gain >= 1.0 - f32::EPSILON && gain <= 1.0 + f32::EPSILON) {
                for i in 0..frames {
                    buf.buf[i] *= gain;
                }
            } // else nothing to do
        }
    }
}

pub struct StereoGainNode<const MAX_BLOCKSIZE: usize> {
    gain_amp: ParamF32<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> StereoGainNode<MAX_BLOCKSIZE> {
    pub fn new(
        gain_db: f32,
        min_db: f32,
        max_db: f32,
        sample_rate: SampleRate,
    ) -> (Self, GainNodeUiHandle) {
        let (gain_amp, gain_handle) = ParamF32::from_value(
            gain_db,
            min_db,
            max_db,
            DB_GRADIENT,
            Unit::Decibels,
            SMOOTH_SECS,
            sample_rate,
        );

        (
            Self { gain_amp },
            GainNodeUiHandle {
                gain_db: gain_handle,
            },
        )
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for StereoGainNode<MAX_BLOCKSIZE>
{
    fn debug_name(&self) -> &'static str {
        "RustyDAWAudioGraph::StereoGain"
    }

    fn stereo_replacing_ports(&self) -> u32 {
        1
    }

    fn process(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: ProcBuffers<f32, MAX_BLOCKSIZE>,
        _global_data: &GlobalData,
    ) {
        if buffers.stereo_replacing.is_empty() {
            return;
        }

        let buf = &mut *buffers.stereo_replacing[0].atomic_borrow_mut();
        let gain_amp = self.gain_amp.smoothed(proc_info.frames);
        let frames = proc_info.frames.compiler_hint_frames();

        // TODO: SIMD

        if gain_amp.is_smoothing() {
            for i in 0..frames {
                buf.left[i] *= gain_amp[i];
                buf.right[i] *= gain_amp[i];
            }
        } else {
            // We can optimize by using a constant gain (better SIMD load efficiency).
            let gain = gain_amp[0];

            if !(gain >= 1.0 - f32::EPSILON && gain <= 1.0 + f32::EPSILON) {
                for i in 0..frames {
                    buf.left[i] *= gain;
                    buf.right[i] *= gain;
                }
            } // else nothing to do
        }
    }
}
