use rusty_daw_core::ProcFrames;

use crate::task::Task;

pub struct Schedule<const MAX_BLOCKSIZE: usize> {
    tasks: Vec<(Task<MAX_BLOCKSIZE>, bool)>,
}

impl<const MAX_BLOCKSIZE: usize> Schedule<MAX_BLOCKSIZE> {
    pub(crate) fn new(tasks: Vec<(Task<MAX_BLOCKSIZE>, bool)>) -> Self {
        Self {
            tasks,
        }
    }

    pub fn process(&mut self, frames: ProcFrames<MAX_BLOCKSIZE>, transport_running: bool) {
        
    }
}

