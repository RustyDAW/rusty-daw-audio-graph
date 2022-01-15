use clap_sys::process::clap_process;

use crate::process::{AudioPorts, ClapPorts, ProcInfo, ProcessStatus};
use crate::rt_processor_pool::SharedRtProcessor;

pub(crate) enum Task<const MAX_BLOCKSIZE: usize> {
    InternalProcessor(InternalProcessorTask<MAX_BLOCKSIZE>),
    ClapProcessor(ClapProcessorTask<MAX_BLOCKSIZE>),
}

impl<const MAX_BLOCKSIZE: usize> std::fmt::Debug for Task<MAX_BLOCKSIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Task::InternalProcessor(t) => {
                let mut f = f.debug_struct("IntProc");

                f.field("id", &t.processor.unique_id());

                match &t.audio_ports {
                    TaskAudioPorts::F32(a) => a.debug_fields(&mut f),
                    TaskAudioPorts::F64(a) => a.debug_fields(&mut f),
                }

                f.finish()
            }
            Task::ClapProcessor(t) => {
                let mut f = f.debug_struct("ClapProc");

                // TODO: Processor ID

                t.ports.debug_fields(&mut f);

                f.finish()
            }
        }
    }
}

pub(crate) enum TaskAudioPorts<const MAX_BLOCKSIZE: usize> {
    F32(AudioPorts<f32, MAX_BLOCKSIZE>),
    F64(AudioPorts<f64, MAX_BLOCKSIZE>),
}

pub(crate) struct InternalProcessorTask<const MAX_BLOCKSIZE: usize> {
    processor: SharedRtProcessor<MAX_BLOCKSIZE>,

    audio_ports: TaskAudioPorts<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> InternalProcessorTask<MAX_BLOCKSIZE> {
    #[inline]
    pub fn process(&mut self, info: &ProcInfo<MAX_BLOCKSIZE>) -> ProcessStatus {
        let Self {
            processor,
            audio_ports,
        } = self;

        let processor = processor.borrow_mut();

        match audio_ports {
            TaskAudioPorts::F32(a) => processor.process(info, a),
            TaskAudioPorts::F64(a) => processor.process_f64(info, a),
        }
    }
}

pub(crate) struct ClapProcessorTask<const MAX_BLOCKSIZE: usize> {
    // TODO: clap processor
    ports: ClapPorts<MAX_BLOCKSIZE>,
}

impl<const MAX_BLOCKSIZE: usize> ClapProcessorTask<MAX_BLOCKSIZE> {
    #[inline]
    pub fn process(&mut self, proc: &mut clap_process) -> ProcessStatus {
        // Prepare the buffers to be sent to the external plugin.
        self.ports.prepare(proc);

        // TODO: process clap plugin

        todo!()
    }
}
