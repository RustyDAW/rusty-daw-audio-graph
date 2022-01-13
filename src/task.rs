use basedrop::Shared;
use clap_sys::process::clap_process;
use std::cell::UnsafeCell;

use crate::audio_buffer::InternalAudioBuffer;
use crate::node::SharedNodeRtProcessor;
use crate::process::{ClapPorts, InternalAudioPorts, ProcInfo, ProcessStatus};

pub(crate) enum Task<const MAX_BLOCKSIZE: usize> {
    InternalNode(InternalNodeTask<MAX_BLOCKSIZE>),
    ClapNode(ClapNodeTask<MAX_BLOCKSIZE>),
}

impl<const MAX_BLOCKSIZE: usize> std::fmt::Debug for Task<MAX_BLOCKSIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Task::InternalNode(n) => {
                let mut f = f.debug_struct("IntNode");

                f.field("id", &n.node.unique_id());

                match &n.audio_ports {
                    TaskAudioPorts::F32(a) => a.debug_fields(&mut f),
                    TaskAudioPorts::F64(a) => a.debug_fields(&mut f),
                }

                f.finish()
            }
            Task::ClapNode(n) => {
                let mut f = f.debug_struct("ClapNode");

                // TODO: Node ID

                n.ports.debug_fields(&mut f);

                f.finish()
            }
        }
    }
}

pub(crate) enum TaskAudioPorts<const MAX_BLOCKSIZE: usize> {
    F32(InternalAudioPorts<f32, MAX_BLOCKSIZE>),
    F64(InternalAudioPorts<f64, MAX_BLOCKSIZE>),
}

pub(crate) struct InternalNodeTask<const MAX_BLOCKSIZE: usize> {
    node: SharedNodeRtProcessor<MAX_BLOCKSIZE>,

    audio_ports: TaskAudioPorts<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> InternalNodeTask<MAX_BLOCKSIZE> {
    #[inline]
    pub fn process(&mut self, info: &ProcInfo<MAX_BLOCKSIZE>) -> ProcessStatus {
        let Self { node, audio_ports } = self;

        let node = node.borrow_mut();

        match audio_ports {
            TaskAudioPorts::F32(a) => node.process(info, a),
            TaskAudioPorts::F64(a) => node.process_f64(info, a),
        }
    }
}

pub(crate) struct ClapNodeTask<const MAX_BLOCKSIZE: usize> {
    // TODO: clap node
    ports: ClapPorts<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> ClapNodeTask<MAX_BLOCKSIZE> {
    #[inline]
    pub fn process(&mut self, proc: &mut clap_process) -> ProcessStatus {
        // Prepare the buffers to be sent to the external plugin.
        self.ports.prepare(proc);

        // TODO: process clap plugin

        todo!()
    }
}
