use basedrop::{Handle, Shared};
use ringbuf::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{AudioGraphNode, ProcBuffers, ProcInfo};

pub struct MonoMonitorNodeHandle {
    pub monitor_rx: Consumer<f32>,
    active: Shared<AtomicBool>,
}

impl MonoMonitorNodeHandle {
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn set_active(&mut self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }
}

pub struct MonoMonitorNode {
    active: Shared<AtomicBool>,
    tx: Producer<f32>,
}

impl MonoMonitorNode {
    pub fn new(
        max_frames: usize,
        active: bool,
        coll_handle: &Handle,
    ) -> (Self, MonoMonitorNodeHandle) {
        let (tx, rx) = RingBuffer::<f32>::new(max_frames).split();

        let active = Shared::new(coll_handle, AtomicBool::new(active));

        (
            Self {
                active: Shared::clone(&active),
                tx,
            },
            MonoMonitorNodeHandle {
                monitor_rx: rx,
                active,
            },
        )
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for MonoMonitorNode
{
    fn debug_name(&self) -> &'static str {
        "MonoMonitorNode"
    }

    fn mono_audio_in_ports(&self) -> u32 {
        1
    }
    fn mono_audio_out_ports(&self) -> u32 {
        1
    }

    fn process(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: &mut ProcBuffers<f32, MAX_BLOCKSIZE>,
        _global_data: &GlobalData,
    ) {
        if buffers.mono_audio_in.is_empty() {
            // As per the spec, all unused audio output buffers must be cleared to 0.0.
            buffers.clear_audio_out_buffers(proc_info);
            return;
        }

        // Won't panic because we checked this was not empty earlier.
        let src = &*buffers.mono_audio_in.buffer(0).unwrap();

        let frames = proc_info.frames();

        if self.active.load(Ordering::Relaxed) {
            self.tx.push_slice(&src.buf[0..frames]);
        }

        if let Some(mut mono_audio_out) = buffers.mono_audio_out.buffer_mut(0) {
            mono_audio_out.copy_frames_from(&*src, frames);
        }
    }
}

pub struct StereoMonitorNodeHandle {
    pub monitor_left_rx: Consumer<f32>,
    pub monitor_right_rx: Consumer<f32>,
    active: Shared<AtomicBool>,
}

impl StereoMonitorNodeHandle {
    pub fn active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn set_active(&mut self, active: bool) {
        self.active.store(active, Ordering::SeqCst);
    }
}

pub struct StereoMonitorNode {
    active: Shared<AtomicBool>,

    left_tx: Producer<f32>,
    right_tx: Producer<f32>,
}

impl StereoMonitorNode {
    pub fn new(
        max_frames: usize,
        active: bool,
        coll_handle: &Handle,
    ) -> (Self, StereoMonitorNodeHandle) {
        let (left_tx, left_rx) = RingBuffer::<f32>::new(max_frames).split();
        let (right_tx, right_rx) = RingBuffer::<f32>::new(max_frames).split();

        let active = Shared::new(coll_handle, AtomicBool::new(active));

        (
            Self {
                active: Shared::clone(&active),
                left_tx,
                right_tx,
            },
            StereoMonitorNodeHandle {
                active,
                monitor_left_rx: left_rx,
                monitor_right_rx: right_rx,
            },
        )
    }
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphNode<GlobalData, MAX_BLOCKSIZE> for StereoMonitorNode
{
    fn debug_name(&self) -> &'static str {
        "StereoMonitorNode"
    }

    fn stereo_audio_in_ports(&self) -> u32 {
        1
    }
    fn stereo_audio_out_ports(&self) -> u32 {
        1
    }

    fn process(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: &mut ProcBuffers<f32, MAX_BLOCKSIZE>,
        _global_data: &GlobalData,
    ) {
        if buffers.stereo_audio_in.is_empty() {
            // As per the spec, all unused audio output buffers must be cleared to 0.0.
            buffers.clear_audio_out_buffers(proc_info);
            return;
        }

        // Won't panic because we checked this was not empty earlier.
        let src = &*buffers.stereo_audio_in.buffer(0).unwrap();

        let frames = proc_info.frames();

        if self.active.load(Ordering::Relaxed) {
            self.left_tx.push_slice(&src.left[0..frames]);
            self.right_tx.push_slice(&src.right[0..frames]);
        }

        if let Some(mut stereo_audio_out) = buffers.stereo_audio_out.buffer_mut(0) {
            stereo_audio_out.copy_frames_from(&*src, frames);
        }
    }
}