//! Dedicated HAL route; CPAL/AudioUnit conversion is not used for DoP.
#[cfg(target_os = "macos")]
mod api;
#[cfg(target_os = "macos")]
mod callback;
#[cfg(target_os = "macos")]
mod context;
#[cfg(target_os = "macos")]
mod device;
mod format;
mod lease;
#[cfg(target_os = "macos")]
pub(super) use callback::Session;
#[cfg(target_os = "macos")]
pub(super) use device::inventory;
