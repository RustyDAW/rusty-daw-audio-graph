use fnv::FnvHashSet;
use std::marker::PhantomData;

use super::resource_pool::{DebugBufferID, DebugNodeID};
use super::task::{AudioGraphTask, MimicProcessReplacingTask};
use super::CompilerError;

pub(crate) struct Verifier<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    /// Used to verify that a node does not appear more than once in the schedule.
    verify_no_duplicate_nodes: FnvHashSet<DebugNodeID>,
    /// Used to verify that a buffer does not appear more than once in the same task.
    verify_no_duplicate_buffers: FnvHashSet<DebugBufferID>,

    _phantom_global: PhantomData<GlobalData>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    Verifier<GlobalData, MAX_BLOCKSIZE>
{
    pub fn new() -> Self {
        Self {
            verify_no_duplicate_nodes: FnvHashSet::default(),
            verify_no_duplicate_buffers: FnvHashSet::default(),
            _phantom_global: PhantomData::default(),
        }
    }

    pub fn verify_no_data_races(
        &mut self,
        tasks: &[AudioGraphTask<GlobalData, MAX_BLOCKSIZE>],
    ) -> Result<(), CompilerError> {
        // TODO: Once we add support for multi-threaded schedules, this function will have
        // the very important task of making sure the schedule contains no data races with
        // the buffers.

        self.verify_no_duplicate_nodes.clear();

        for task in tasks.iter() {
            self.verify_no_duplicate_buffers.clear();

            match task {
                AudioGraphTask::Node(node_task) => {
                    // Verify that a node does not appear more than once in the schedule.
                    if !self
                        .verify_no_duplicate_nodes
                        .insert(node_task.node.debug_id())
                    {
                        return Err(CompilerError::NodeAppearsTwice(node_task.node.debug_id()));
                    }

                    // Very that a buffer does not appear more than once in the same task.
                    for buf in node_task.proc_buffer_assignment.mono_replacing.iter() {
                        if !self.verify_no_duplicate_buffers.insert(buf.debug_id()) {
                            return Err(CompilerError::BufferAppearsTwice(buf.debug_id()));
                        }
                    }
                    for buf in node_task.proc_buffer_assignment.indep_mono_in.iter() {
                        if !self.verify_no_duplicate_buffers.insert(buf.debug_id()) {
                            return Err(CompilerError::BufferAppearsTwice(buf.debug_id()));
                        }
                    }
                    for buf in node_task.proc_buffer_assignment.indep_mono_out.iter() {
                        if !self.verify_no_duplicate_buffers.insert(buf.debug_id()) {
                            return Err(CompilerError::BufferAppearsTwice(buf.debug_id()));
                        }
                    }
                    for buf in node_task.proc_buffer_assignment.stereo_replacing.iter() {
                        if !self.verify_no_duplicate_buffers.insert(buf.debug_id()) {
                            return Err(CompilerError::BufferAppearsTwice(buf.debug_id()));
                        }
                    }
                    for buf in node_task.proc_buffer_assignment.indep_stereo_in.iter() {
                        if !self.verify_no_duplicate_buffers.insert(buf.debug_id()) {
                            return Err(CompilerError::BufferAppearsTwice(buf.debug_id()));
                        }
                    }
                    for buf in node_task.proc_buffer_assignment.indep_stereo_out.iter() {
                        if !self.verify_no_duplicate_buffers.insert(buf.debug_id()) {
                            return Err(CompilerError::BufferAppearsTwice(buf.debug_id()));
                        }
                    }
                }
                AudioGraphTask::MimicProcessReplacing(step) => match step {
                    MimicProcessReplacingTask::CopyMonoBuffers(copy_task) => {
                        for (buf_1, buf_2) in copy_task.iter() {
                            // Make sure that these two buffers aren't the same buffer.
                            if buf_1.debug_id() == buf_2.debug_id() {
                                return Err(CompilerError::BufferAppearsTwice(buf_1.debug_id()));
                            }
                        }
                    }
                    MimicProcessReplacingTask::CopyStereoBuffers(copy_task) => {
                        for (buf_1, buf_2) in copy_task.iter() {
                            // Make sure that these two buffers aren't the same buffer.
                            if buf_1.debug_id() == buf_2.debug_id() {
                                return Err(CompilerError::BufferAppearsTwice(buf_1.debug_id()));
                            }
                        }
                    }
                    MimicProcessReplacingTask::ClearMonoBuffers(_clear_task) => {
                        // Nothing to do (until we have multithreaded schedules).
                    }
                    MimicProcessReplacingTask::ClearStereoBuffers(_clear_task) => {
                        // Nothing to do (until we have multithreaded schedules).
                    }
                },
            }
        }

        Ok(())
    }
}
