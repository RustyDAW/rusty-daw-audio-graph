use crate::{AudioGraphNode, ProcBuffers, ProcInfo};

pub struct MonoSampleDelayNode {
    buf: Vec<f32>,
    read_pointer: usize,
}

impl MonoSampleDelayNode {
    pub fn new(delay: u32) -> Self {
        Self {
            buf: vec![0.0; delay as usize],
            read_pointer: 0,
        }
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for MonoSampleDelayNode
{
    fn debug_name(&self) -> &'static str {
        "MonoSampleDelayNode"
    }

    fn mono_audio_in_ports(&self) -> u32 {
        1
    }
    fn mono_audio_out_ports(&self) -> u32 {
        1
    }

    fn delay(&self) -> u32 {
        self.buf.len() as u32
    }

    fn process(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: &mut ProcBuffers<f32, MAX_BLOCKSIZE>,
        _global_data: &GlobalData,
    ) {
        if buffers.mono_audio_in.is_empty() || buffers.mono_audio_out.is_empty() {
            // As per the spec, all unused audio output buffers must be cleared to 0.0.
            buffers.clear_audio_out_buffers(proc_info);
            return;
        }

        // Won't panic because we checked these were not empty earlier.
        let src = &*buffers.mono_audio_in.buffer(0).unwrap();
        let dst = &mut *buffers.mono_audio_out.buffer_mut(0).unwrap();

        let frames = proc_info.frames();

        // TODO: Check that the compiler elids all bounds checking properly. If not, then raw unsafe memcpys could
        // possibly be used if more performance is needed.

        if frames > self.buf.len() {
            if self.read_pointer == 0 {
                // Only one copy is needed.

                // Copy all frames from self.buf into the output buffer.
                dst[0..self.buf.len()].copy_from_slice(&self.buf[0..self.buf.len()]);
            } else if self.read_pointer < self.buf.len() {
                // This check will always be true, it is here to hint to the compiler to optimize.
                // Two copies are needed.

                let first_len = self.buf.len() - self.read_pointer;

                // Copy frames from self.buf into the output buffer.
                dst[0..first_len].copy_from_slice(&self.buf[self.read_pointer..self.buf.len()]);
                dst[first_len..self.buf.len()].copy_from_slice(&self.buf[0..self.read_pointer]);
            }

            // Copy the remaining frames from the input buffer to the output buffer.
            let remaining = frames - self.buf.len();
            dst.buf[self.buf.len()..frames].copy_from_slice(&src.buf[0..remaining]);

            // Copy the final remaining frames from the input buffer into self.buf.
            // self.buf is "empty" at this point, so reset the read pointer so only one copy operation is needed.
            self.read_pointer = 0;
            let buf_len = self.buf.len();
            self.buf[0..buf_len].copy_from_slice(&src[remaining..frames]);
        } else {
            if self.read_pointer + frames < self.buf.len() {
                // Only one copy is needed.

                // Copy frames from self.buf into the output buffer.
                dst[0..frames]
                    .copy_from_slice(&self.buf[self.read_pointer..self.read_pointer + frames]);

                // Copy all frames from the input buffer into self.buf.
                self.buf[self.read_pointer..self.read_pointer + frames]
                    .copy_from_slice(&src.buf[0..frames]);
            } else {
                // Two copies are needed.

                let first_len = self.buf.len() - self.read_pointer;
                let second_len = frames - first_len;

                // Copy frames from self.buf into the output buffer.
                dst[0..first_len].copy_from_slice(&self.buf[self.read_pointer..self.buf.len()]);
                dst[first_len..frames].copy_from_slice(&self.buf[0..second_len]);

                // Copy all frames from the input buffer into self.buf.
                let buf_len = self.buf.len();
                self.buf[self.read_pointer..buf_len].copy_from_slice(&src.buf[0..first_len]);
                self.buf[0..second_len].copy_from_slice(&src.buf[first_len..frames]);
            }

            // Get the next position of the read pointer.
            self.read_pointer += frames;
            if self.read_pointer >= self.buf.len() {
                self.read_pointer -= self.buf.len();
            }
        }
    }
}

pub struct StereoSampleDelayNode {
    buf_left: Vec<f32>,
    buf_right: Vec<f32>,
    read_pointer: usize,
}

impl StereoSampleDelayNode {
    pub fn new(delay: u32) -> Self {
        Self {
            buf_left: vec![0.0; delay as usize],
            buf_right: vec![0.0; delay as usize],
            read_pointer: 0,
        }
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for StereoSampleDelayNode
{
    fn debug_name(&self) -> &'static str {
        "StereoSampleDelayNode"
    }

    fn stereo_audio_in_ports(&self) -> u32 {
        1
    }
    fn stereo_audio_out_ports(&self) -> u32 {
        1
    }

    fn delay(&self) -> u32 {
        self.buf_left.len() as u32
    }

    fn process(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: &mut ProcBuffers<f32, MAX_BLOCKSIZE>,
        _global_data: &GlobalData,
    ) {
        // TODO: Check that the compiler elids all bounds checking properly. If not, then raw unsafe memcpys could
        // possibly be used if more performance is needed.

        // This is always true. It is here to hint to the compiler to elid bounds checking.
        if self.buf_left.len() != self.buf_right.len() {
            return;
        }

        if buffers.stereo_audio_in.is_empty() || buffers.stereo_audio_out.is_empty() {
            // As per the spec, all unused audio output buffers must be cleared to 0.0.
            buffers.clear_audio_out_buffers(proc_info);
            return;
        }

        // Won't panic because we checked these were not empty earlier.
        let src = &*buffers.stereo_audio_in.buffer(0).unwrap();
        let dst = &mut *buffers.stereo_audio_out.buffer_mut(0).unwrap();

        let frames = proc_info.frames();

        if frames > self.buf_left.len() {
            if self.read_pointer == 0 {
                // Only one copy is needed.

                // Copy all frames from self.buf into the output buffer.
                dst.left[0..self.buf_left.len()]
                    .copy_from_slice(&self.buf_left[0..self.buf_left.len()]);
                dst.right[0..self.buf_left.len()]
                    .copy_from_slice(&self.buf_right[0..self.buf_left.len()]);
            } else if self.read_pointer < self.buf_left.len() {
                // This check will always be true, it is here to hint to the compiler to optimize.
                // Two copies are needed.

                let first_len = self.buf_left.len() - self.read_pointer;

                // Copy frames from self.buf into the output buffer.
                dst.left[0..first_len]
                    .copy_from_slice(&self.buf_left[self.read_pointer..self.buf_left.len()]);
                dst.left[first_len..self.buf_left.len()]
                    .copy_from_slice(&self.buf_left[0..self.read_pointer]);

                dst.right[0..first_len]
                    .copy_from_slice(&self.buf_right[self.read_pointer..self.buf_left.len()]);
                dst.right[first_len..self.buf_left.len()]
                    .copy_from_slice(&self.buf_right[0..self.read_pointer]);
            }

            // Copy the remaining frames from the input buffer to the output buffer.
            let remaining = frames - self.buf_left.len();
            dst.left[self.buf_left.len()..frames].copy_from_slice(&src.left[0..remaining]);
            dst.right[self.buf_left.len()..frames].copy_from_slice(&src.right[0..remaining]);

            // Copy the final remaining frames from the input buffer into self.buf.
            // self.buf is "empty" at this point, so reset the read pointer so only one copy operation is needed.
            self.read_pointer = 0;
            let buf_len = self.buf_left.len();
            self.buf_left[0..buf_len].copy_from_slice(&src.left[remaining..frames]);
            self.buf_right[0..buf_len].copy_from_slice(&src.right[remaining..frames]);
        } else {
            if self.read_pointer + frames < self.buf_left.len() {
                // Only one copy is needed.

                // Copy frames from self.buf into the output buffer.
                dst.left[0..frames]
                    .copy_from_slice(&self.buf_left[self.read_pointer..self.read_pointer + frames]);
                dst.right[0..frames].copy_from_slice(
                    &self.buf_right[self.read_pointer..self.read_pointer + frames],
                );

                // Copy all frames from the input buffer into self.buf.
                self.buf_left[self.read_pointer..self.read_pointer + frames]
                    .copy_from_slice(&src.left[0..frames]);
                self.buf_right[self.read_pointer..self.read_pointer + frames]
                    .copy_from_slice(&src.right[0..frames]);
            } else {
                // Two copies are needed.

                let first_len = self.buf_left.len() - self.read_pointer;
                let second_len = frames - first_len;

                // Copy frames from self.buf into the output buffer.
                dst.left[0..first_len]
                    .copy_from_slice(&self.buf_left[self.read_pointer..self.buf_left.len()]);
                dst.left[first_len..frames].copy_from_slice(&self.buf_left[0..second_len]);

                dst.right[0..first_len]
                    .copy_from_slice(&self.buf_right[self.read_pointer..self.buf_left.len()]);
                dst.right[first_len..frames].copy_from_slice(&self.buf_right[0..second_len]);

                // Copy all frames from the input buffer into self.buf.
                let buf_len = self.buf_left.len();
                self.buf_left[self.read_pointer..buf_len].copy_from_slice(&src.left[0..first_len]);
                self.buf_left[0..second_len].copy_from_slice(&src.left[first_len..frames]);

                self.buf_right[self.read_pointer..buf_len]
                    .copy_from_slice(&src.right[0..first_len]);
                self.buf_right[0..second_len].copy_from_slice(&src.right[first_len..frames]);
            }

            // Get the next position of the read pointer.
            self.read_pointer += frames;
            if self.read_pointer >= self.buf_left.len() {
                self.read_pointer -= self.buf_left.len();
            }
        }
    }
}
