//! Exact software representations of original-depth integer PCM.
//!
//! These adapters describe and check a caller-selected output format. They do
//! not negotiate a device or establish that its mixer, driver or DAC preserves
//! the programme. Encoding runs outside the real-time callback and never uses
//! the DSP session, output guard or quantizer.

use std::collections::TryReserveError;
use std::fmt;

use aede_dsp::ChannelLayout;

use super::format::{IntegerPcmFormat, PcmStreamFormat};

/// Maximum number of complete frames encoded by one bounded block operation.
pub const MAX_EXACT_BLOCK_FRAMES: usize = 4096;

/// An explicit little-endian, interleaved output representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactSampleRepresentation {
    /// Full-width signed 16-bit PCM.
    Signed16Le,
    /// Full-width signed 24-bit PCM packed into three bytes per sample.
    PackedSigned24Le,
    /// Full-width signed 32-bit PCM, including exact left-aligned widening.
    Signed32Le,
    /// Normalized IEEE 754 binary32 samples.
    Float32Le,
    /// Normalized IEEE 754 binary64 samples.
    Float64Le,
}

impl ExactSampleRepresentation {
    /// Number of bytes occupied by one encoded channel sample.
    pub fn bytes_per_sample(self) -> usize {
        match self {
            Self::Signed16Le => 2,
            Self::PackedSigned24Le => 3,
            Self::Signed32Le | Self::Float32Le => 4,
            Self::Float64Le => 8,
        }
    }

    fn signed_container_bits(self) -> Option<u32> {
        match self {
            Self::Signed16Le => Some(16),
            Self::PackedSigned24Le => Some(24),
            Self::Signed32Le => Some(32),
            Self::Float32Le | Self::Float64Le => None,
        }
    }
}

/// Explicit output facts supplied by a caller, without device certification.
///
/// `effective_bits` is the known downstream signed-PCM precision, not the byte
/// width of the transport. For example, a normalized float transport can feed
/// a known 24-bit destination. Unknown precision can be retained for diagnosis
/// but is refused when constructing an exact adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactOutputFormat {
    pcm: PcmStreamFormat,
    representation: ExactSampleRepresentation,
    effective_bits: Option<u32>,
}

impl ExactOutputFormat {
    /// Validate explicit rate, speaker positions and precision metadata.
    ///
    /// Effective precision must be between 1 and 64 bits when known, and must
    /// fit a signed integer transport's container. A float representation does
    /// not by itself prove the precision of any subsequent conversion.
    pub fn new(
        sample_rate: u32,
        layout: ChannelLayout,
        representation: ExactSampleRepresentation,
        effective_bits: Option<u32>,
    ) -> Result<Self, ExactOutputError> {
        if layout.mask().is_none() {
            return Err(ExactOutputError::InvalidOutputFormat);
        }
        let pcm = PcmStreamFormat::with_layout(sample_rate, layout)
            .map_err(|_| ExactOutputError::InvalidOutputFormat)?;
        if let Some(bits) = effective_bits
            && (!(1..=64).contains(&bits)
                || representation
                    .signed_container_bits()
                    .is_some_and(|container| bits > container))
        {
            return Err(ExactOutputError::InvalidEffectivePrecision(bits));
        }
        Ok(Self {
            pcm,
            representation,
            effective_bits,
        })
    }

    /// Explicit output frames per second.
    pub fn sample_rate(self) -> u32 {
        self.pcm.sample_rate()
    }

    /// Explicit interleaved speaker positions.
    pub fn layout(self) -> ChannelLayout {
        self.pcm.layout()
    }

    /// Number of interleaved samples in each output frame.
    pub fn channels(self) -> u16 {
        self.pcm.channels()
    }

    /// Selected transport representation and byte order.
    pub fn representation(self) -> ExactSampleRepresentation {
        self.representation
    }

    /// Caller-supplied downstream precision; no measurement is implied.
    pub fn effective_bits(self) -> Option<u32> {
        self.effective_bits
    }
}

/// A refusal to represent source samples exactly in a selected output format.
#[derive(Debug)]
pub enum ExactOutputError {
    /// Rate or channel positions are invalid or unknown.
    InvalidOutputFormat,
    /// Claimed effective precision is invalid for its representation.
    InvalidEffectivePrecision(u32),
    /// The downstream precision has not been established by the caller.
    UnknownEffectivePrecision,
    /// Exact output cannot resample to another rate.
    RateMismatch {
        /// Original source frames per second.
        source: u32,
        /// Selected output frames per second.
        output: u32,
    },
    /// Exact output cannot mix or remap source speaker positions.
    LayoutMismatch {
        /// Original source speaker positions and interleaved order.
        source: ChannelLayout,
        /// Selected output speaker positions and interleaved order.
        output: ChannelLayout,
    },
    /// The representation or downstream precision would discard source bits.
    InsufficientPrecision {
        /// Original signed-PCM valid bit depth.
        source: u32,
        /// Available effective precision after representation constraints.
        output: u32,
    },
    /// A source block ends inside an interleaved frame.
    IncompleteFrame,
    /// A source block exceeds the bounded encoding operation.
    BlockTooLarge,
    /// A canonical source integer is outside its declared valid-bit range.
    SampleOutOfRange {
        /// Zero-based interleaved sample position in the submitted block.
        index: usize,
        /// Invalid canonical source integer.
        sample: i32,
        /// Declared signed-PCM valid bit depth.
        bits: u32,
    },
    /// The caller's encoded buffer cannot reserve the needed capacity.
    Allocation(TryReserveError),
}

impl fmt::Display for ExactOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOutputFormat => {
                f.write_str("exact output needs a positive rate and explicit speaker positions")
            }
            Self::InvalidEffectivePrecision(bits) => {
                write!(f, "invalid exact-output effective precision: {bits} bits")
            }
            Self::UnknownEffectivePrecision => {
                f.write_str("exact output needs known downstream precision")
            }
            Self::RateMismatch { source, output } => write!(
                f,
                "exact output cannot change the source rate from {source} to {output} Hz"
            ),
            Self::LayoutMismatch { source, output } => write!(
                f,
                "exact output cannot change the source layout from {} to {}",
                source.name(),
                output.name()
            ),
            Self::InsufficientPrecision { source, output } => write!(
                f,
                "exact output cannot preserve {source}-bit source PCM with {output} effective bits"
            ),
            Self::IncompleteFrame => {
                f.write_str("exact source block must contain complete interleaved frames")
            }
            Self::BlockTooLarge => write!(
                f,
                "exact source block exceeds {MAX_EXACT_BLOCK_FRAMES} frames"
            ),
            Self::SampleOutOfRange {
                index,
                sample,
                bits,
            } => write!(
                f,
                "exact source sample {index} ({sample}) is outside the signed {bits}-bit range"
            ),
            Self::Allocation(error) => write!(f, "cannot reserve exact-output block: {error}"),
        }
    }
}

impl std::error::Error for ExactOutputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

/// A validated, stateless mapping from canonical source integers to bytes.
///
/// No source rate, precision, speaker position or normalized sample value is
/// changed. Output integers are full-width PCM: widening shifts source values
/// towards the most significant bit, rather than merely sign-extending them.
/// Float scaling is exact for every source value in the 16/24-bit profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactPcmAdapter {
    source: IntegerPcmFormat,
    output: ExactOutputFormat,
}

impl ExactPcmAdapter {
    /// Refuse rate/layout conversion and unknown or insufficient precision.
    pub fn new(
        source: IntegerPcmFormat,
        output: ExactOutputFormat,
    ) -> Result<Self, ExactOutputError> {
        if source.sample_rate() != output.sample_rate() {
            return Err(ExactOutputError::RateMismatch {
                source: source.sample_rate(),
                output: output.sample_rate(),
            });
        }
        if source.layout() != output.layout() {
            return Err(ExactOutputError::LayoutMismatch {
                source: source.layout(),
                output: output.layout(),
            });
        }
        let effective = output
            .effective_bits()
            .ok_or(ExactOutputError::UnknownEffectivePrecision)?;
        let available = output
            .representation()
            .signed_container_bits()
            .map_or(effective, |container| effective.min(container));
        if available < source.bits_per_sample() {
            return Err(ExactOutputError::InsufficientPrecision {
                source: source.bits_per_sample(),
                output: available,
            });
        }
        Ok(Self { source, output })
    }

    /// Original unscaled signed source format accepted by this adapter.
    pub fn source_format(self) -> IntegerPcmFormat {
        self.source
    }

    /// Software output representation accepted by this adapter.
    pub fn output_format(self) -> ExactOutputFormat {
        self.output
    }

    /// Replace `encoded` with one bounded, complete interleaved block.
    ///
    /// Return the encoded frame count. An empty block produces no bytes. All
    /// source samples, frame alignment and capacity are validated before the
    /// existing encoded bytes are modified. Allocation is permitted here, so
    /// callers must encode outside the output callback and may reuse capacity.
    pub fn encode_block(
        self,
        samples: &[i32],
        encoded: &mut Vec<u8>,
    ) -> Result<usize, ExactOutputError> {
        let channels = usize::from(self.source.channels());
        if !samples.len().is_multiple_of(channels) {
            return Err(ExactOutputError::IncompleteFrame);
        }
        let frames = samples.len() / channels;
        if frames > MAX_EXACT_BLOCK_FRAMES {
            return Err(ExactOutputError::BlockTooLarge);
        }
        let bits = self.source.bits_per_sample();
        let negative_limit = -(1 << (bits - 1));
        let positive_limit = (1 << (bits - 1)) - 1;
        for (index, &sample) in samples.iter().enumerate() {
            if !(negative_limit..=positive_limit).contains(&sample) {
                return Err(ExactOutputError::SampleOutOfRange {
                    index,
                    sample,
                    bits,
                });
            }
        }
        // The frame bound and mono/stereo source admission make this product
        // bounded by 65536 bytes, independently of caller-owned vector size.
        let bytes = samples.len() * self.output.representation().bytes_per_sample();
        encoded
            .try_reserve(bytes.saturating_sub(encoded.len()))
            .map_err(ExactOutputError::Allocation)?;
        encoded.clear();
        for &sample in samples {
            match self.output.representation() {
                ExactSampleRepresentation::Signed16Le => {
                    encoded.extend_from_slice(&(sample as i16).to_le_bytes());
                }
                ExactSampleRepresentation::PackedSigned24Le => {
                    let widened = sample << (24 - bits);
                    encoded.extend_from_slice(&widened.to_le_bytes()[..3]);
                }
                ExactSampleRepresentation::Signed32Le => {
                    encoded.extend_from_slice(&(sample << (32 - bits)).to_le_bytes());
                }
                ExactSampleRepresentation::Float32Le => {
                    let normalized = sample as f32 / (1_u32 << (bits - 1)) as f32;
                    encoded.extend_from_slice(&normalized.to_le_bytes());
                }
                ExactSampleRepresentation::Float64Le => {
                    let normalized = f64::from(sample) / f64::from(1_u32 << (bits - 1));
                    encoded.extend_from_slice(&normalized.to_le_bytes());
                }
            }
        }
        Ok(frames)
    }
}

#[cfg(test)]
#[path = "exact_output_tests.rs"]
mod tests;
