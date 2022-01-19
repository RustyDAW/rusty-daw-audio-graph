use ringbuf::{Consumer, Producer, RingBuffer};

use crate::plugin_instance_pool::{UniquePluginInstanceID, UniqueProcessorID};

/// Information about the host
pub struct HostInfo {
    /// The name of this host (this is mandatory)
    ///
    /// eg: "Meadowlark"
    pub name: &'static str,

    /// The version of this host (this is mandatory)
    ///
    /// A quick way to do this is to set this equal to `env!("CARGO_PKG_VERSION")`
    /// to automatically update this when your crate version changes.
    ///
    /// eg: "1.4.2", "1.0.2_beta"
    pub version: &'static str,

    /// The vendor of this host (this is optional)
    ///
    /// eg: "RustyDAW Org"
    pub vendor: Option<&'static str>,

    /// The url to the product page for this host (this is optional)
    ///
    /// eg: "https://meadowlark.app"
    pub url: Option<&'static str>,
}

impl HostInfo {
    /// Get the version number of the "rusty daw engine" that this
    /// host is using.
    pub fn rusty_daw_engine_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HostRequest {
    pub process_id: UniqueProcessorID,
    pub plugin_instance_id: UniquePluginInstanceID,
    pub request_type: HostRequestType,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HostRequestType {
    Restart,
    Process,
    Callback,
}

/// 1,000 plugin instances * 3 * sizeof(HostRequest)(24 bytes) per slot = 72KB
///
/// Still, this is probably overkill as it's extremely unlikely that *every* plugin
/// instance will use all 3 slots at once, but memory is cheap nowadays and better
/// safe than sorry.
pub(crate) const INITIAL_PLUGIN_INSTANCE_QUEUE_COUNT: usize = 1000 * 3;

/// Used to retrieve info about the host as well as requesting actions from the
/// host
pub struct RtHostQueue {
    info: HostInfo,

    /// This is set before calling `process()` on each rt processor.
    ids: Option<(UniqueProcessorID, UniquePluginInstanceID)>,

    requests_tx: Producer<HostRequest>,
}

impl RtHostQueue {
    pub(crate) fn new(info: HostInfo, requests_tx: Producer<HostRequest>) -> Self {
        Self {
            info,
            ids: None,
            requests_tx,
        }
    }

    #[inline]
    pub(crate) fn prepare_for_processor(
        &mut self,
        ids: Option<(UniqueProcessorID, UniquePluginInstanceID)>,
    ) {
        self.ids = ids;
    }

    /// Used by the scheduler to send a larger buffer if the current one is too small.
    pub(crate) fn use_new_request_buffer(&mut self, requests_tx: Producer<HostRequest>) {
        self.requests_tx = requests_tx;
    }

    /// Retrieve info about the host
    pub fn info(&self) -> &HostInfo {
        &self.info
    }

    /// Request the host to deactivate and then reactivate this plugin.
    ///
    /// This operation may be delayed by the host.
    ///
    /// Please call this only once per plugin instance when it is actually
    /// need so as to avoid duplicate requests.
    pub fn request_restart(&mut self) {
        if let Some((process_id, plugin_instance_id)) = self.ids {
            if let Err(_) = self.requests_tx.push(HostRequest {
                process_id,
                plugin_instance_id,
                request_type: HostRequestType::Restart,
            }) {
                // In theory this should not happen because the scheduler sends a larger message buffer
                // if the number of plugin instances exceeds `INITIAL_PLUGIN_INSTANCE_QUEUE_COUNT / 3` (1,000).

                log::error!("Plugin could not request restart from host, message queue is full");
            }
        }
    }

    /// Request the host to activate and start processing the plugiin.
    ///
    /// This is useful if you have external IO and need to wake up the plugin
    /// from "sleep"
    ///
    /// Please call this only once per plugin instance when it is actually
    /// need so as to avoid duplicate requests.
    pub fn request_process(&mut self) {
        if let Some((process_id, plugin_instance_id)) = self.ids {
            if let Err(_) = self.requests_tx.push(HostRequest {
                process_id,
                plugin_instance_id,
                request_type: HostRequestType::Process,
            }) {
                // In theory this should not happen because the scheduler sends a larger message buffer
                // if the number of plugin instances exceeds `INITIAL_PLUGIN_INSTANCE_QUEUE_COUNT / 3` (1,000).

                log::error!("Plugin could not request restart from host, message queue is full");
            }
        }
    }

    /// Request the host to schedule a call to `Plugin::on_main_thread()` on
    /// the main thread.
    ///
    /// Please call this only once per plugin instance when it is actually
    /// need so as to avoid duplicate requests.
    pub fn request_callback(&mut self) {
        if let Some((process_id, plugin_instance_id)) = self.ids {
            if let Err(_) = self.requests_tx.push(HostRequest {
                process_id,
                plugin_instance_id,
                request_type: HostRequestType::Callback,
            }) {
                // In theory this should not happen because the scheduler sends a larger message buffer
                // if the number of plugin instances exceeds `INITIAL_PLUGIN_INSTANCE_QUEUE_COUNT / 3` (1,000).

                log::error!("Plugin could not request restart from host, message queue is full");
            }
        }
    }
}
