use rusty_daw_core::{Gradient, ParamF32, ParamF32UiHandle, SampleRate, Unit};

use super::{DB_GRADIENT, SMOOTH_SECS};
use crate::{AudioGraphNode, ProcBuffers, ProcInfo};

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanLaw {
    Linear,
}

pub struct StereoGainPainUiHandle {
    pub gain_db: ParamF32UiHandle,
    pub pan: ParamF32UiHandle,

    pan_law: PanLaw,
}

impl StereoGainPainUiHandle {
    pub fn pan_law(&self) -> &PanLaw {
        &self.pan_law
    }
}

pub struct StereoGainPanNode<const MAX_BLOCKSIZE: usize> {
    gain_amp: ParamF32<MAX_BLOCKSIZE>,
    pan: ParamF32<MAX_BLOCKSIZE>,
    pan_law: PanLaw,
}

impl<const MAX_BLOCKSIZE: usize> StereoGainPanNode<MAX_BLOCKSIZE> {
    pub fn new(
        gain_db: f32,
        min_db: f32,
        max_db: f32,
        pan: f32,
        pan_law: PanLaw,
        sample_rate: SampleRate,
    ) -> (Self, StereoGainPainUiHandle) {
        let (gain_amp, gain_handle) = ParamF32::from_value(
            gain_db,
            min_db,
            max_db,
            DB_GRADIENT,
            Unit::Decibels,
            SMOOTH_SECS,
            sample_rate,
        );

        let (pan, pan_handle) = ParamF32::from_value(
            pan,
            0.0,
            1.0,
            Gradient::Linear,
            Unit::Generic,
            SMOOTH_SECS,
            sample_rate,
        );

        (
            Self {
                gain_amp,
                pan,
                pan_law,
            },
            StereoGainPainUiHandle {
                gain_db: gain_handle,
                pan: pan_handle,
                pan_law,
            },
        )
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for StereoGainPanNode<MAX_BLOCKSIZE>
{
    fn debug_name(&self) -> &'static str {
        "RustyDAWAudioGraph::StereoGainPan"
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
        let frames = proc_info.frames.compiler_hint_min(MAX_BLOCKSIZE);
        let gain_amp = self.gain_amp.smoothed(frames);
        let pan = self.pan.smoothed(frames);

        // TODO: SIMD

        if pan.is_smoothing() {
            // Need to calculate left and right gain per sample.
            match self.pan_law {
                PanLaw::Linear => {
                    // TODO: I'm not sure this is actually linear pan-law. I'm just getting something down for now.

                    if gain_amp.is_smoothing() {
                        for i in 0..frames.0 {
                            buf.left[i] *= (1.0 - pan.values[i]) * gain_amp.values[i];
                            buf.right[i] *= pan.values[i] * gain_amp.values[i];
                        }
                    } else {
                        // We can optimize by using a constant gain (better SIMD load efficiency).
                        let gain = gain_amp.values[0];

                        for i in 0..frames.0 {
                            buf.left[i] *= (1.0 - pan.values[i]) * gain;
                            buf.right[i] *= pan.values[i] * gain;
                        }
                    }
                }
            }
        } else {
            // We can optimize by only calculating left and right gain once.
            let (left_amp, right_amp) = match self.pan_law {
                PanLaw::Linear => {
                    // TODO: I'm not sure this is actually linear pan-law. I'm just getting something down for now.
                    (1.0 - pan.values[0], pan.values[0])
                }
            };

            if gain_amp.is_smoothing() {
                for i in 0..frames.0 {
                    buf.left[i] *= left_amp * gain_amp.values[i];
                    buf.right[i] *= right_amp * gain_amp.values[i];
                }
            } else {
                // We can optimize by pre-multiplying gain to the pan.
                let left_amp = left_amp * gain_amp.values[0];
                let right_amp = right_amp * gain_amp.values[0];

                if !(left_amp >= 1.0 - f32::EPSILON && left_amp <= 1.0 + f32::EPSILON)
                    || !(right_amp >= 1.0 - f32::EPSILON && right_amp <= 1.0 + f32::EPSILON)
                {
                    for i in 0..frames.0 {
                        buf.left[i] *= left_amp;
                        buf.right[i] *= right_amp;
                    }
                } // else nothing to do
            }
        }
    }
}
