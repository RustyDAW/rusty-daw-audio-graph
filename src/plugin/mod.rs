use rusty_daw_core::SampleRate;
use std::error::Error;

pub mod audio_ports;

use crate::process::{AudioPorts, ProcInfo, ProcessStatus};

pub struct PluginDescriptor {
    /// The unique reverse-domain-name identifier of this plugin.
    ///
    /// eg: "org.rustydaw.spicysynth"
    pub id: String,

    /// The displayable name of this plugin.
    ///
    /// eg: "Spicy Synth"
    pub name: String,

    /// The vendor of this plugin.
    ///
    /// eg: "RustyDAW"
    pub vendor: String,

    /// The version of this plugin.
    ///
    /// eg: "1.4.4" or "1.1.2_beta"
    pub version: String,

    /// A displayable short description of this plugin.
    ///
    /// eg: "Create flaming-hot sounds!"
    pub description: String,

    /// Arbitrary list of keywords, separated by `;'.
    ///
    /// They can be matched by the host search engine and used to classify the plugin.
    ///
    /// Some pre-defined keywords:
    /// - "instrument", "audio_effect", "note_effect", "analyzer"
    /// - "mono", "stereo", "surround", "ambisonic"
    /// - "distortion", "compressor", "limiter", "transient"
    /// - "equalizer", "filter", "de-esser"
    /// - "delay", "reverb", "chorus", "flanger"
    /// - "tool", "utility", "glitch"
    ///
    /// Some examples:
    /// - "equalizer;analyzer;stereo;mono"
    /// - "compressor;analog;character;mono"
    /// - "reverb;plate;stereo"
    pub features: Option<String>,

    /// The url to the product page of this plugin.
    ///
    /// Set to `None` if there is no product page.
    pub url: Option<String>,

    /// The url to the online manual for this plugin.
    ///
    /// Set to `None` if there is no online manual.
    pub manual_url: Option<String>,

    /// The url to the online support page for this plugin.
    ///
    /// Set to `None` if there is no online support page.
    pub support_url: Option<String>,
}

pub trait Plugin<const MAX_BLOCKSIZE: usize> {
    /// Return the description of this plugin.
    fn descriptor(&self) -> PluginDescriptor;

    /// Activate this plugin, and return the realtime processor counterpart of
    /// this plugin instance.
    ///
    /// This won't be called again until after the host calls `deactivate`.
    ///
    /// In this call the plugin may allocate memory and prepare everything needed
    /// for the process call. Once this is called the latency and port configuration
    /// must remain constant, until deactivation.
    ///
    /// The given sample rate will remain constant for the entire lifetime of
    /// this instance.
    fn activate(
        &mut self,
        sample_rate: SampleRate,
    ) -> Result<Box<dyn RtProcessor<MAX_BLOCKSIZE>>, Box<dyn Error>>;

    /// Called when the plugin becomes inactive (the realtime processor counterpart gets
    /// dropped).
    fn deactive(&mut self) {}

    /// An optional extension that describes the available audio ports
    /// on this plugin.
    ///
    /// This will only be called while the plugin is inactive.
    ///
    /// When `None` is returned then the plugin will have a default
    /// of one stereo input and one stereo output port. (Also the host
    /// may use the same buffer for these ports ("process replacing").)
    ///
    /// By default this returns `None`.
    fn audio_ports_extension<'a>(&'a self) -> Option<audio_ports::PluginAudioPortsExtension<'a>> {
        None
    }

    /// TODO
    ///
    /// Called by the host on the main thread in response to a previous request
    /// by this plugin.
    fn on_main_thread(&mut self) {}
}

pub trait RtProcessor<const MAX_BLOCKSIZE: usize>: Send {
    /// Called before the host starts processing/wakes the plugin from
    /// sleep.
    ///
    /// This will always be called once before the first call to `process()`.
    ///
    /// Note, this will be called on the realtime-thread, so only
    /// realtime-safe things are allowed in this method.
    fn start_processing(&mut self) {}

    /// Called before the host sends the plugin to sleep.
    ///
    /// Note, this will be called on the realtime-thread, so only
    /// realtime-safe things are allowed in this method.
    fn stop_processing(&mut self) {}

    /// The main process method (`f32` buffers).
    ///
    /// The host may call this method even if your plugin has set
    /// `preferred_sample_size` to `F64`. In that case it is up to
    /// you to decide whether to process in `f32` or convert to/from
    /// `f64` buffers manually.
    fn process(
        &mut self,
        info: &ProcInfo<MAX_BLOCKSIZE>,
        audio: &mut AudioPorts<f32, MAX_BLOCKSIZE>,
    ) -> ProcessStatus;

    /// The main process method (`f64` buffers).
    ///
    /// The host will not call this method unless your plugin has
    /// set `preferred_sample_size` to `F64`.
    #[allow(unused)]
    fn process_f64(
        &mut self,
        info: &ProcInfo<MAX_BLOCKSIZE>,
        audio: &mut AudioPorts<f64, MAX_BLOCKSIZE>,
    ) -> ProcessStatus {
        ProcessStatus::Error
    }
}
