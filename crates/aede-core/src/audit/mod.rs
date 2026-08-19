//! What a file really contains, as opposed to what it claims.
//!
//! Tag reading answers "what does this file say about itself". This module
//! answers "is it telling the truth", by walking the encoded stream without
//! decoding any audio.
//!
//! It deliberately depends on nothing but [`crate::tags`], so it can be lifted
//! into a crate of its own and shared with other tools.

mod bits;
mod crc;
pub mod flac;
pub mod integrity;

pub use flac::{FlacAudit, Limits, StereoContent};
