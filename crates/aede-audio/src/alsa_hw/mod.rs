//! Shared direct-ALSA route admission for exact PCM and original DSD output.
//! Pure observations are validation inputs, not independent hardware proof.
mod controls;
#[cfg(all(target_os = "linux", feature = "dop-alsa"))]
mod device;
mod selection;
pub use controls::{ControlKind, ControlMap, channel_layout, neutral_control, verify_control_map};
#[cfg(all(target_os = "linux", feature = "dop-alsa"))]
pub use device::open_controls;
pub use selection::{Selection, hardware_name, matches_identity, selection};
