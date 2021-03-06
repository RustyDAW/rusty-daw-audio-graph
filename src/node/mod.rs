use std::fmt::Debug;

use super::schedule::ProcInfo;
use super::ProcBuffers;

pub mod gain;
pub mod monitor;
pub mod pan;
pub mod sample_delay;
pub mod sum;

use rusty_daw_core::{Gradient, Seconds};

pub const SMOOTH_SECS: Seconds = Seconds(5.0 / 1_000.0);
pub const DB_GRADIENT: Gradient = Gradient::Power(0.15);

/// A node in an audio graph.
pub trait AudioGraphNode<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize>:
    Send + Sync
{
    /// The name of this node. This is used for debugging purposes.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    fn debug_name(&self) -> &'static str;

    /// The number of available mono audio replacing ports.
    ///
    /// "Replacing" ports are a single pair of input/output ports that share the same buffer,
    /// equivalent to the concept of `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "replacing" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP.
    ///
    /// These ports do **not** count towards those defined in `indep_mono_in_ports`
    /// and `indep_mono_out_ports`.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    ///
    /// By default, this returns 0 (no replacing ports)
    fn mono_replacing_ports(&self) -> u32 {
        0
    }

    /// The number of "independent" mono audio input ports.
    ///
    /// "Independent" means that this input port is **not** a "Replacing" port. "Replacing" ports are a
    /// single pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "replacing" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP. If your port has this "process_replacing()" quality,
    /// please add it using `mono_replacing_ports()` instead for better efficiency.
    ///
    /// These are distinct from ports defined in `mono_replacing_ports()`.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    ///
    /// By default, this returns 0 (no ports)
    fn indep_mono_in_ports(&self) -> u32 {
        0
    }

    /// The number of "independent" mono audio output ports.
    ///
    /// "Independent" means that this output port is **not** a "Replacing" port. "Replacing" ports are a
    /// single pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "replacing" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP. If your port has this "process_replacing()" quality,
    /// please add it using `mono_replacing_ports()` instead for better efficiency.
    ///
    /// These are distinct from ports defined in `mono_replacing_ports()`.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    ///
    /// Also please note that these buffers may contain junk data, so please do ***NOT*** read
    /// from these buffers before writing to them. Also, as per the spec, if you do not end up
    /// using this buffer, then it **MUST** be manually cleared to 0.0 before returning your
    /// `process` method.
    ///
    /// By default, this returns 0 (no ports)
    fn indep_mono_out_ports(&self) -> u32 {
        0
    }

    /// The number of available stereo audio replacing ports.
    ///
    /// "Replacing" ports are a single pair of input/output ports that share the same buffer,
    /// equivalent to the concept of `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "replacing" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP.
    ///
    /// These ports do **not** count towards those defined in `indep_stereo_in_ports`
    /// and `indep_stereo_out_ports`.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    ///
    /// By default, this returns 0 (no replacing ports)
    fn stereo_replacing_ports(&self) -> u32 {
        0
    }

    /// The number of "independent" stereo audio input ports.
    ///
    /// "Independent" means that this input port is **not** a "Replacing" port. "Replacing" ports are a
    /// single pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "replacing" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP. If your port has this "process_replacing()" quality,
    /// please add it using `stereo_replacing_ports()` instead for better efficiency.
    ///
    /// These are distinct from ports defined in `stereo_replacing_ports()`.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    ///
    /// By default, this returns 0 (no ports)
    fn indep_stereo_in_ports(&self) -> u32 {
        0
    }

    /// The number of "independent" stereo audio output ports.
    ///
    /// "Independent" means that this output port is **not** a "Replacing" port. "Replacing" ports are a
    /// single pair of input/output ports that share the same buffer, equivalent to the concept of
    /// `process_replacing()` in VST2.
    ///
    /// Note that the scheduler will *always* use "replacing" ports if they are available, event if it
    /// has to copy input/output buffers behind the scenes. So no need to add a separate
    /// "non-process-replacing" version of your DSP. If your port has this "process_replacing()" quality,
    /// please add it using `stereo_replacing_ports()` instead for better efficiency.
    ///
    /// These are distinct from ports defined in `stereo_replacing_ports()`.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the number of ports, the intended workflow is
    /// to "replace" the existing node with another one with the desired number of ports.
    ///
    /// Also please note that these buffers may contain junk data, so please do ***NOT*** read
    /// from these buffers before writing to them. Also, as per the spec, if you do not end up
    /// using this buffer, then it **MUST** be manually cleared to 0.0 before returning your
    /// `process` method.
    ///
    /// By default, this returns 0 (no ports)
    fn indep_stereo_out_ports(&self) -> u32 {
        0
    }

    /// The delay in audio frames that this node produces.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change the delay, the intended workflow is to "replace"
    /// the existing node with another one that has the desired delay.
    ///
    /// By default, this returns 0 (no delay)
    fn delay(&self) -> u32 {
        0
    }

    /*
    /// Whether or not this node supports processing with `f64` audio buffers.
    ///
    /// Even though this takes `self` as an argument, this ***must*** remain constant for the entire
    /// lifetime of this node. If you wish to change this, the intended workflow is to "replace" the
    /// existing node with another one that has the desired value.
    ///
    /// By default, this returns `false`.
    fn supports_f64(&self) -> bool {
        false
    }
    */

    /// Process the given buffers.
    ///
    /// The number of buffers may be less than the number of ports on this node. In that case it
    /// just means some ports are disconnected.
    ///
    /// In addition, the `sample_rate` and `sample_rate_recip` (1.0 / sample_rate) of the stream
    /// is given. These will remain constant for the lifetime of this node, so these are just provided
    /// for convinience.
    fn process<'a>(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: ProcBuffers<'a, f32, MAX_BLOCKSIZE>,
        global_data: &GlobalData,
    );

    /*
    /// Process the given buffers.
    #[allow(unused_variables)]
    fn process_f64<'a>(
        &mut self,
        proc_info: &ProcInfo<MAX_BLOCKSIZE>,
        buffers: ProcBuffers<'a, f64, MAX_BLOCKSIZE>,
        global_data: &GlobalData,
    ) {
    }
    */
}

// Lets us use unwrap.
impl<GlobalData: Send + Sync + 'static, const MAX_BLOCKSIZE: usize> Debug
    for Box<dyn AudioGraphNode<GlobalData, MAX_BLOCKSIZE>>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Audio Graph Node")
    }
}
