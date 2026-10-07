//! Shared processing format and original-precision integer source metadata.
//!
//! Local and remote playback use one rate/channel type, including when they
//! pass decoded blocks to processing, meters, or an output sink. The separate
//! integer source format retains valid precision before any such conversion.

pub use aede_dsp::{DspError as FormatError, PcmFormat as PcmStreamFormat};

/// Original signed integer PCM in the initial exact-decoding profile.
///
/// Samples use an `i32` storage container but are canonical, unscaled source
/// integers: 24-bit `1` remains `1`, not a left-aligned 32-bit value. Valid bit
/// depth is therefore distinct from the container width. This format carries
/// no output-device negotiation or bit-perfect device guarantee.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegerPcmFormat {
    pcm: PcmStreamFormat,
    bits_per_sample: u32,
}

impl IntegerPcmFormat {
    /// Validate the initial 16/24-bit mono/stereo source profile.
    pub fn new(
        sample_rate: u32,
        layout: aede_dsp::ChannelLayout,
        bits_per_sample: u32,
    ) -> Result<Self, FormatError> {
        if !matches!(bits_per_sample, 16 | 24)
            || !matches!(
                layout,
                aede_dsp::ChannelLayout::MONO | aede_dsp::ChannelLayout::STEREO
            )
        {
            return Err(FormatError::InvalidFormat);
        }
        Ok(Self {
            pcm: PcmStreamFormat::with_layout(sample_rate, layout)?,
            bits_per_sample,
        })
    }

    /// Source frames per second, without rate conversion.
    pub fn sample_rate(self) -> u32 {
        self.pcm.sample_rate()
    }

    /// Number of original interleaved samples in a frame.
    pub fn channels(self) -> u16 {
        self.pcm.channels()
    }

    /// Valid source precision, independent of the `i32` storage width.
    pub fn bits_per_sample(self) -> u32 {
        self.bits_per_sample
    }

    /// Original mono or front-left/front-right stereo association.
    pub fn layout(self) -> aede_dsp::ChannelLayout {
        self.pcm.layout()
    }
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
