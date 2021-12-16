use crate::{AudioGraphNode, ProcBuffers, ProcInfo};

pub struct MonoSumNode {
    num_inputs: u32,
}

impl MonoSumNode {
    pub fn new(num_inputs: u32) -> Self {
        Self { num_inputs }
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for MonoSumNode
{
    fn debug_name(&self) -> &'static str {
        "RustyDAWAudioGraph::MonoSum"
    }

    // The first port is a "replacing" port for efficiency. All the rest
    // are "independent" ports.
    fn mono_replacing_ports(&self) -> u32 {
        if self.num_inputs == 0 {
            0
        } else {
            1
        }
    }
    fn indep_mono_in_ports(&self) -> u32 {
        if self.num_inputs == 0 {
            0
        } else {
            self.num_inputs - 1
        }
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

        let frames = proc_info.frames.compiler_hint_frames();
        // Won't panic because we checked this was not empty earlier.
        let replacing = &mut *buffers.mono_replacing[0].atomic_borrow_mut();
        let audio_in = &buffers.indep_mono_in;

        // TODO: SIMD

        match audio_in.len() {
            0 => return,
            1 => {
                let src_1 = &*audio_in[0].atomic_borrow();

                for i in 0..frames {
                    replacing[i] += src_1[i];
                }
            }
            2 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();

                for i in 0..frames {
                    replacing[i] += src_1[i] + src_2[i];
                }
            }
            3 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();

                for i in 0..frames {
                    replacing[i] += src_1[i] + src_2[i] + src_3[i];
                }
            }
            4 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();

                for i in 0..frames {
                    replacing[i] += src_1[i] + src_2[i] + src_3[i] + src_4[i];
                }
            }
            5 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();
                let src_5 = &*audio_in[4].atomic_borrow();

                for i in 0..frames {
                    replacing[i] += src_1[i] + src_2[i] + src_3[i] + src_4[i] + src_5[i];
                }
            }
            6 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();
                let src_5 = &*audio_in[4].atomic_borrow();
                let src_6 = &*audio_in[5].atomic_borrow();

                for i in 0..frames {
                    replacing[i] += src_1[i] + src_2[i] + src_3[i] + src_4[i] + src_5[i] + src_6[i];
                }
            }
            7 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();
                let src_5 = &*audio_in[4].atomic_borrow();
                let src_6 = &*audio_in[5].atomic_borrow();
                let src_7 = &*audio_in[6].atomic_borrow();

                for i in 0..frames {
                    replacing[i] +=
                        src_1[i] + src_2[i] + src_3[i] + src_4[i] + src_5[i] + src_6[i] + src_7[i];
                }
            }
            // TODO: Additional optimized loops?
            num_inputs => {
                for ch_i in 1..num_inputs {
                    let src = &*audio_in[ch_i].atomic_borrow();

                    for i in 0..frames {
                        replacing[i] += src[i];
                    }
                }
            }
        }
    }
}

pub struct StereoSumNode {
    num_stereo_inputs: u32,
}

impl StereoSumNode {
    pub fn new(num_stereo_inputs: u32) -> Self {
        Self { num_stereo_inputs }
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for StereoSumNode
{
    fn debug_name(&self) -> &'static str {
        "RustyDAWAudioGraph::StereoSum"
    }

    // The first port is a "replacing" port for efficiency. All the rest
    // are "independent" ports.
    fn stereo_replacing_ports(&self) -> u32 {
        if self.num_stereo_inputs == 0 {
            0
        } else {
            1
        }
    }
    fn indep_stereo_in_ports(&self) -> u32 {
        if self.num_stereo_inputs == 0 {
            0
        } else {
            self.num_stereo_inputs - 1
        }
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

        let frames = proc_info.frames.compiler_hint_frames();
        // Won't panic because we checked this was not empty earlier.
        let mut replacing = buffers.stereo_replacing[0].atomic_borrow_mut();
        let audio_in = &buffers.indep_stereo_in;

        // TODO: SIMD

        match audio_in.len() {
            0 => return,
            1 => {
                let src_1 = &*audio_in[0].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] += src_1.left[i];
                    replacing.right[i] += src_1.right[i];
                }
            }
            2 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] += src_1.left[i] + src_2.left[i];
                    replacing.right[i] += src_1.right[i] + src_2.right[i];
                }
            }
            3 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] += src_1.left[i] + src_2.left[i] + src_3.left[i];
                    replacing.right[i] += src_1.right[i] + src_2.right[i] + src_3.right[i];
                }
            }
            4 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] +=
                        src_1.left[i] + src_2.left[i] + src_3.left[i] + src_4.left[i];
                    replacing.right[i] +=
                        src_1.right[i] + src_2.right[i] + src_3.right[i] + src_4.right[i];
                }
            }
            5 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();
                let src_5 = &*audio_in[4].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] += src_1.left[i]
                        + src_2.left[i]
                        + src_3.left[i]
                        + src_4.left[i]
                        + src_5.left[i];
                    replacing.right[i] += src_1.right[i]
                        + src_2.right[i]
                        + src_3.right[i]
                        + src_4.right[i]
                        + src_5.right[i];
                }
            }
            6 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();
                let src_5 = &*audio_in[4].atomic_borrow();
                let src_6 = &*audio_in[5].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] += src_1.left[i]
                        + src_2.left[i]
                        + src_3.left[i]
                        + src_4.left[i]
                        + src_5.left[i]
                        + src_6.left[i];
                    replacing.right[i] += src_1.right[i]
                        + src_2.right[i]
                        + src_3.right[i]
                        + src_4.right[i]
                        + src_5.right[i]
                        + src_6.right[i];
                }
            }
            7 => {
                let src_1 = &*audio_in[0].atomic_borrow();
                let src_2 = &*audio_in[1].atomic_borrow();
                let src_3 = &*audio_in[2].atomic_borrow();
                let src_4 = &*audio_in[3].atomic_borrow();
                let src_5 = &*audio_in[4].atomic_borrow();
                let src_6 = &*audio_in[5].atomic_borrow();
                let src_7 = &*audio_in[6].atomic_borrow();

                for i in 0..frames {
                    replacing.left[i] += src_1.left[i]
                        + src_2.left[i]
                        + src_3.left[i]
                        + src_4.left[i]
                        + src_5.left[i]
                        + src_6.left[i]
                        + src_7.left[i];
                    replacing.right[i] += src_1.right[i]
                        + src_2.right[i]
                        + src_3.right[i]
                        + src_4.right[i]
                        + src_5.right[i]
                        + src_6.right[i]
                        + src_7.right[i];
                }
            }
            // TODO: Additional optimized loops?
            num_stereo_inputs => {
                for ch_i in 1..num_stereo_inputs {
                    let src = &*audio_in[ch_i].atomic_borrow();

                    for i in 0..frames {
                        replacing.left[i] += src.left[i];
                        replacing.right[i] += src.right[i];
                    }
                }
            }
        }
    }
}
