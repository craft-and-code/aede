//! Compatibility names for the PCM format owned by `aede-dsp`.
//!
//! Local and remote playback use one rate/channel type, including when they
//! pass decoded blocks to processing, meters, or an output sink.

pub use aede_dsp::{DspError as FormatError, PcmFormat as PcmStreamFormat};

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
