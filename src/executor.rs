use atomic_refcell::{AtomicRefCell, AtomicRefMut};
use basedrop::{Handle, Shared, SharedCell};
use rusty_daw_core::SampleRate;

use crate::resource_pool::GraphResourcePool;
use crate::schedule::Schedule;

pub struct AudioGraphExecutor<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> {
    pub(crate) schedule: AtomicRefCell<Schedule<GlobalData, MAX_BLOCKSIZE>>,
    pub(crate) global_data: Shared<AtomicRefCell<GlobalData>>,
}

impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>
    AudioGraphExecutor<GlobalData, MAX_BLOCKSIZE>
{
    pub(crate) fn new(
        coll_handle: Handle,
        sample_rate: SampleRate,
        global_data: GlobalData,
    ) -> (
        Shared<SharedCell<AudioGraphExecutor<GlobalData, MAX_BLOCKSIZE>>>,
        GraphResourcePool<GlobalData, MAX_BLOCKSIZE>,
    ) {
        let mut resource_pool = GraphResourcePool::new(coll_handle.clone());

        let root_out_buffer = resource_pool.get_temp_stereo_audio_block_buffer(0);

        (
            Shared::new(
                &coll_handle,
                SharedCell::new(Shared::new(
                    &coll_handle,
                    AudioGraphExecutor {
                        schedule: AtomicRefCell::new(Schedule::new(
                            vec![],
                            sample_rate,
                            root_out_buffer,
                        )),
                        global_data: Shared::new(&coll_handle, AtomicRefCell::new(global_data)),
                    },
                )),
            ),
            resource_pool,
        )
    }

    #[cfg(not(feature = "cpal-backend"))]
    pub fn process<G: FnMut(AtomicRefMut<GlobalData>, usize)>(
        &self,
        mut out: &mut [f32],
        mut global_data_process: G,
    ) {
        // Should not panic because the non-rt thread always creates a new schedule every time before sending
        // it to the rt thread via a SharedCell.
        let schedule = &mut *AtomicRefCell::borrow_mut(&self.schedule);

        // Assume output is stereo for now.
        let mut frames_left = out.len() / 2;

        // Process in blocks.
        while frames_left > 0 {
            let frames = frames_left.min(MAX_BLOCKSIZE);

            // Process the user's global data. This should not panic because this is the only place
            // this is ever borrowed.
            {
                let global_data = AtomicRefCell::borrow_mut(&self.global_data);
                global_data_process(global_data, frames);
            }

            {
                let global_data = AtomicRefCell::borrow(&self.global_data);
                schedule.process(frames, global_data);
            }

            schedule.from_root_output_interleaved(&mut out[0..(frames * 2)]);

            out = &mut out[(frames * 2)..];
            frames_left -= frames;
        }
    }

    #[cfg(feature = "cpal-backend")]
    pub fn process<T: cpal::Sample, G: FnMut(AtomicRefMut<GlobalData>, usize)>(
        &self,
        mut out: &mut [T],
        mut global_data_process: G,
    ) {
        // Should not panic because the non-rt thread always creates a new schedule every time before sending
        // it to the rt thread via a SharedCell.
        let schedule = &mut *AtomicRefCell::borrow_mut(&self.schedule);

        // Assume output is stereo for now.
        let mut frames_left = out.len() / 2;

        // Process in blocks.
        while frames_left > 0 {
            let frames = frames_left.min(MAX_BLOCKSIZE);

            // Process the user's global data. This should not panic because this is the only place
            // this is ever borrowed.
            {
                let global_data = AtomicRefCell::borrow_mut(&self.global_data);
                global_data_process(global_data, frames);
            }

            {
                let global_data = AtomicRefCell::borrow(&self.global_data);
                schedule.process(frames, global_data);
            }

            schedule.from_root_output_interleaved(&mut out[0..(frames * 2)]);

            out = &mut out[(frames * 2)..];
            frames_left -= frames;
        }
    }
}
