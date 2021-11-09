use std::fmt;

use smallvec::SmallVec;

use crate::proc_buffers::ProcBufferAssignment;
use crate::shared::{SharedMonoBuffer, SharedNode, SharedStereoBuffer};
use crate::SMALLVEC_ALLOC_MPR_BUFFERS;

#[non_exhaustive]
pub(crate) enum AudioGraphTask<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    Node(AudioGraphNodeTask<GlobalData, MAX_BLOCKSIZE>),
    MimicProcessReplacing(MimicProcessReplacingTask<f32, MAX_BLOCKSIZE>),
}

/// This contains specialized tasks that are necessary in faking "process_replacing" behavior
/// when the scheduler cannot assign a single buffer to both the input and output. This is
/// necessary because this audio graph spec gaurantees that nodes that request "process_replacing"
/// behavior will always get this behavior (so the user doesn't need to have two separate versions
/// of their DSP code).
#[non_exhaustive]
pub(crate) enum MimicProcessReplacingTask<T: Default + Copy + Clone, const MAX_BLOCKSIZE: usize> {
    CopyMonoBuffers(
        SmallVec<
            [(
                SharedMonoBuffer<T, MAX_BLOCKSIZE>,
                SharedMonoBuffer<T, MAX_BLOCKSIZE>,
            ); SMALLVEC_ALLOC_MPR_BUFFERS],
        >,
    ),
    CopyStereoBuffers(
        SmallVec<
            [(
                SharedStereoBuffer<T, MAX_BLOCKSIZE>,
                SharedStereoBuffer<T, MAX_BLOCKSIZE>,
            ); SMALLVEC_ALLOC_MPR_BUFFERS],
        >,
    ),
    ClearMonoBuffers(SmallVec<[SharedMonoBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_MPR_BUFFERS]>),
    ClearStereoBuffers(
        SmallVec<[SharedStereoBuffer<T, MAX_BLOCKSIZE>; SMALLVEC_ALLOC_MPR_BUFFERS]>,
    ),
}

pub(crate) struct AudioGraphNodeTask<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
{
    pub node: SharedNode<GlobalData, MAX_BLOCKSIZE>,
    pub proc_buffer_assignment: ProcBufferAssignment<f32, MAX_BLOCKSIZE>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> fmt::Debug
    for AudioGraphTask<GlobalData, MAX_BLOCKSIZE>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AudioGraphTask::Node(task) => {
                let mut mono_replacing = String::new();
                let mut indep_mono_in = String::new();
                let mut indep_mono_out = String::new();
                let mut stereo_replacing = String::new();
                let mut indep_stereo_in = String::new();
                let mut indep_stereo_out = String::new();

                for b in task.proc_buffer_assignment.mono_replacing.iter() {
                    mono_replacing.push_str(&format!("[{:?}], ", b));
                }
                for b in task.proc_buffer_assignment.indep_mono_in.iter() {
                    indep_mono_in.push_str(&format!("[{:?}], ", b));
                }
                for b in task.proc_buffer_assignment.indep_mono_out.iter() {
                    indep_mono_out.push_str(&format!("[{:?}], ", b));
                }
                for b in task.proc_buffer_assignment.stereo_replacing.iter() {
                    stereo_replacing.push_str(&format!("[{:?}], ", b));
                }
                for b in task.proc_buffer_assignment.indep_mono_in.iter() {
                    indep_stereo_in.push_str(&format!("[{:?}], ", b));
                }
                for b in task.proc_buffer_assignment.indep_stereo_out.iter() {
                    indep_stereo_out.push_str(&format!("[{:?}], ", b));
                }

                let mut ds = f.debug_struct(&format!("Node: {:?}", task.node));

                if !mono_replacing.is_empty() {
                    ds.field("mono_replacing", &mono_replacing);
                }
                if !indep_mono_in.is_empty() {
                    ds.field("indep_mono_in", &indep_mono_in);
                }
                if !indep_mono_out.is_empty() {
                    ds.field("indep_mono_out", &indep_mono_out);
                }
                if !stereo_replacing.is_empty() {
                    ds.field("stereo_replacing", &stereo_replacing);
                }
                if !indep_stereo_in.is_empty() {
                    ds.field("indep_stereo_in", &indep_stereo_in);
                }
                if !indep_stereo_out.is_empty() {
                    ds.field("indep_stereo_out", &indep_stereo_out);
                }

                ds.finish()
            }
            AudioGraphTask::MimicProcessReplacing(step) => match step {
                MimicProcessReplacingTask::CopyMonoBuffers(task) => {
                    let copy_mono_buffers: Vec<String> = task
                        .iter()
                        .map(|b| format!("[src: ({:?}), dst: ({:?})], ", b.0, b.1))
                        .collect();
                    f.debug_struct("MimicProcessReplacing::CopyMonoBuffers")
                        .field("copy_buffers", &copy_mono_buffers)
                        .finish()
                }
                MimicProcessReplacingTask::CopyStereoBuffers(task) => {
                    let copy_stereo_buffers: Vec<String> = task
                        .iter()
                        .map(|b| format!("[src: ({:?}), dst: ({:?})], ", b.0, b.1))
                        .collect();
                    f.debug_struct("MimicProcessReplacing::CopyStereoBuffers")
                        .field("copy_buffers", &copy_stereo_buffers)
                        .finish()
                }
                MimicProcessReplacingTask::ClearMonoBuffers(task) => {
                    let clear_mono_buffers: Vec<String> =
                        task.iter().map(|b| format!("[{:?}], ", b)).collect();
                    f.debug_struct("MimicProcessReplacing::ClearMonoBuffers")
                        .field("clear_buffers", &clear_mono_buffers)
                        .finish()
                }
                MimicProcessReplacingTask::ClearStereoBuffers(task) => {
                    let clear_stereo_buffers: Vec<String> =
                        task.iter().map(|b| format!("[{:?}], ", b)).collect();
                    f.debug_struct("MimicProcessReplacing::ClearStereoBuffers")
                        .field("clear_buffers", &clear_stereo_buffers)
                        .finish()
                }
            },
        }
    }
}
