//! What a file really contains, as opposed to what it claims.
//!
//! Tag reading reports declarations. Structural stream walks check supported
//! container checksums and encoded audio properties. FLAC precision candidates
//! additionally use the existing integer codec for bounded confirmation from
//! already-read frames; the integrity/checksum walk does not decode audio.

mod bits;
/// Public because a checksum is a checksum. `copy` verifies a file it has just
/// written with the same CRC-32 routine Ogg pages are checked with, rather than
/// growing a second implementation of the same polynomial next door.
pub mod crc;
pub mod flac;
pub mod integrity;

pub use flac::{FlacAudit, Limits, StereoContent};

#[cfg(test)]
mod test_support;
