// Shared product schema and validation used by both desktop entries and the builder CLI.
pub mod error;
mod handshake;
pub mod hardware;
pub mod input;
pub mod model;
pub mod product;
pub mod profile;
mod serial;
mod storage;

#[cfg(feature = "product-studio")]
pub mod product_build;
#[cfg(feature = "product-studio")]
mod studio;

#[cfg(not(feature = "product-studio"))]
mod app;
#[cfg(not(feature = "product-studio"))]
mod coordinator;
#[cfg(not(feature = "product-studio"))]
mod device;
#[cfg(not(feature = "product-studio"))]
#[allow(dead_code)]
mod display;
#[cfg(not(feature = "product-studio"))]
mod metrics;
#[cfg(not(feature = "product-studio"))]
mod paste;
#[cfg(not(feature = "product-studio"))]
mod protocol;
#[cfg(not(feature = "product-studio"))]
mod runtime_log;
#[cfg(all(
    not(feature = "product-studio"),
    any(target_os = "macos", target_os = "windows")
))]
mod tray;
#[cfg(not(feature = "product-studio"))]
#[allow(dead_code)]
mod trigger;
#[cfg(not(feature = "product-studio"))]
mod workspace;

#[cfg(not(feature = "product-studio"))]
#[doc(hidden)]
pub mod test_support {
    pub use crate::{
        coordinator::{
            BootloaderObservation, ConnectionDimension, DeviceMode, RuntimeCoordinator,
            RuntimeDimension, SerialObservation, UsbEnumerator, WorkspaceRevision,
        },
        device::{SerialTransport, SerialTransportFactory, SystemWorkerLauncher},
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        paste::{ClipboardWriter, Clock, PasteCoordinator},
        profile::{
            ButtonAction, DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION,
            TriggerActions, TriggerSettings,
        },
        workspace::{RuntimeAssignment, Workspace},
    };

    pub fn wait_for_paste_request(
        paste: &PasteCoordinator,
        device_id: &crate::hardware::DeviceId,
        event_id: u64,
        step: u16,
        text: &str,
        timeout: std::time::Duration,
    ) -> Result<(), String> {
        paste.wait_for_request(device_id, event_id, step, text, timeout)
    }
}

#[cfg(not(feature = "product-studio"))]
pub use app::run;
#[cfg(feature = "product-studio")]
pub use studio::run;
