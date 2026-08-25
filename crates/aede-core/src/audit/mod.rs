//! What a file really contains, as opposed to what it claims.
//!
//! Tag reading answers "what does this file say about itself". This module
//! answers "is it telling the truth", by walking the encoded stream without
//! decoding any audio.
//!
//! It deliberately depends on nothing but [`crate::tags`], so it can be lifted
//! into a crate of its own and shared with other tools.

mod bits;
/// Public because a checksum is a checksum. `copy` verifies a file it has just
/// written with the same CRC-32 routine Ogg pages are checked with, rather than
/// growing a second implementation of the same polynomial next door.
pub mod crc;
pub mod flac;
pub mod integrity;

pub use flac::{FlacAudit, Limits, StereoContent};
