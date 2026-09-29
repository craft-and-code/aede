//! Format metadata shared by local and remote PCM consumers.

use std::fmt;

/// The rate and channel count of interleaved floating-point PCM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmStreamFormat {
    sample_rate: u32,
    channels: u16,
}

impl PcmStreamFormat {
    /// Refuse formats without a valid frame rate or channel count.
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self, FormatError> {
        if sample_rate == 0 || channels == 0 {
            return Err(FormatError);
        }
        Ok(Self {
            sample_rate,
            channels,
        })
    }

    /// Samples per second on every channel.
    pub fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved samples in each frame.
    pub fn channels(self) -> u16 {
        self.channels
    }
}

/// The PCM stream format cannot represent audio frames.
#[derive(Debug)]
pub struct FormatError;

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("audio output needs a positive sample rate and channel count")
    }
}

impl std::error::Error for FormatError {}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
