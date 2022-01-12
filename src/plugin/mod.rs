pub mod audio_ports;

pub trait Plugin {
    /// An optional extension that describes the available audio ports
    /// on this plugin.
    ///
    /// This will only be called while the plugin is inactive.
    ///
    /// When `None` is returned then the plugin will have a default
    /// of one stereo input and one stereo output port. (Also the host
    /// may use the same buffer for these ports)
    ///
    /// By default this returns `None`.
    fn plugin_audio_ports_extension<'a>(
        &'a self,
    ) -> Option<audio_ports::PluginAudioPortsExtension<'a>> {
        None
    }
}
