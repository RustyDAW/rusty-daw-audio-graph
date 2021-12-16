use basedrop::{Handle, Shared};
use ringbuf::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{AudioGraphNode, ProcBuffers, ProcInfo};

pub struct MonoMonitorNodeUiHandle {
    pub monitor_rx: Consumer<f32>,
    active: Shared<AtomicBool>,
}

impl MonoMonitorNodeUiHandle {
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
    ) -> (Self, MonoMonitorNodeUiHandle) {
        let (tx, rx) = RingBuffer::<f32>::new(max_frames).split();

        let active = Shared::new(coll_handle, AtomicBool::new(active));

        (
            Self {
                active: Shared::clone(&active),
                tx,
            },
            MonoMonitorNodeUiHandle {
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
        "RustyDAWAudioGraph::MonoMonitor"
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
        if self.active.load(Ordering::Relaxed) && !buffers.mono_replacing.is_empty() {
            let buf = buffers.mono_replacing[0].atomic_borrow();

            self.tx
                .push_slice(&buf.buf[0..proc_info.frames.unchecked_frames()]);
        }
    }
}

pub struct StereoMonitorNodeUiHandle {
    pub monitor_left_rx: Consumer<f32>,
    pub monitor_right_rx: Consumer<f32>,
    active: Shared<AtomicBool>,
}

impl StereoMonitorNodeUiHandle {
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
    ) -> (Self, StereoMonitorNodeUiHandle) {
        let (left_tx, left_rx) = RingBuffer::<f32>::new(max_frames).split();
        let (right_tx, right_rx) = RingBuffer::<f32>::new(max_frames).split();

        let active = Shared::new(coll_handle, AtomicBool::new(active));

        (
            Self {
                active: Shared::clone(&active),
                left_tx,
                right_tx,
            },
            StereoMonitorNodeUiHandle {
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
        "RustyDAWAudioGraph::StereoMonitor"
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
        if self.active.load(Ordering::Relaxed) && !buffers.stereo_replacing.is_empty() {
            let buf = buffers.stereo_replacing[0].atomic_borrow();

            self.left_tx
                .push_slice(&buf.left[0..proc_info.frames.unchecked_frames()]);
            self.right_tx
                .push_slice(&buf.right[0..proc_info.frames.unchecked_frames()]);
        }
    }
}
