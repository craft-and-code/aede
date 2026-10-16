//! Walks the FLAC frame structure, with bounded precision confirmation.
//!
//! Two claims a file makes about itself cannot be checked from its header
//! alone: how many bits of resolution it really carries, and whether its two
//! channels hold different sound. Both answers are already written in the
//! encoded stream. The structural walk finds candidates; padding claims also
//! need the exact integer samples after stereo decorrelation.
//!
//! **Resolution.** When the low bits of every sample are zero — the signature
//! of 16-bit audio re-encoded as 24-bit — the encoder does not store them. It
//! records a *wasted bits* count in each subframe instead. A file claiming 24
//! bits with eight wasted bits everywhere may carry 16 bits of real music.
//! Correlated and constant subframes can hide significant original low bits,
//! so a lazy existing decoder confirms candidates one bounded frame at a
//! time, from the bytes already read. An unavailable confirmation makes no
//! padding claim. The checksum walk remains entirely structural.
//!
//! **Channels.** For stereo, the encoder picks the cheapest of four
//! correlations per frame. When both channels are identical the difference
//! channel is constant zero, which shows up as a `CONSTANT` subframe.
//!
//! An encoder that skipped these optimisations yields no additional evidence.
//! Precision confirmation can withdraw a structural padding candidate; it
//! does not certify the file's integrity or establish the recording's origin.
//!
//! Reference: <https://xiph.org/flac/format.html>

use std::path::Path;

use super::bits::BitReader;
use crate::tags::TagError;
use crate::tags::bytes::read_at_most;

#[path = "flac_precision.rs"]
mod precision;

/// How much of a file the walk is allowed to read.
///
/// The structural walk and optional single-frame precision confirmation must
/// stay bounded over a large library. Padding is a property of the whole
/// file; the quick result describes only the frames examined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Bytes of encoded audio to read at most.
    pub max_bytes: usize,
    /// Frames to walk at most.
    pub max_frames: usize,
}

impl Limits {
    /// Enough to answer confidently while staying cheap over a whole library:
    /// 512 frames is roughly 47 seconds of CD-quality audio.
    pub fn quick() -> Limits {
        Limits {
            max_bytes: 8 * 1024 * 1024,
            max_frames: 512,
        }
    }

    /// Reads the file to its end. For inspecting one file on demand.
    pub fn thorough() -> Limits {
        Limits {
            max_bytes: usize::MAX,
            max_frames: usize::MAX,
        }
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits::quick()
    }
}

/// What the frame structure says about the audio, beyond what the header
/// claims.
#[derive(Debug, Clone, PartialEq)]
pub struct FlacAudit {
    /// Bit depth written in STREAMINFO.
    pub declared_bit_depth: u16,
    /// Conservative bit-depth estimate for the frames examined, once
    /// confirmed wasted low bits are removed. Unavailable confirmation retains
    /// the declared depth.
    ///
    /// Lower than `declared_bit_depth` means the file was padded: 24-bit
    /// packaging around 16-bit music.
    pub effective_bit_depth: u16,
    /// Channel count written in STREAMINFO.
    pub channels: u16,
    /// What the two channels of a stereo file actually hold.
    pub stereo: StereoContent,
    /// Whether every subframe examined was constant zero, which means silence.
    pub digital_silence: bool,
    /// Number of frames the walk covered.
    pub frames_examined: usize,
    /// `true` when a limit stopped the walk before the end of the file.
    ///
    /// The verdict then describes the portion examined, not the whole file.
    pub truncated: bool,
}

impl FlacAudit {
    /// `true` when the file claims more resolution than it carries.
    pub fn is_padded(&self) -> bool {
        self.effective_bit_depth < self.declared_bit_depth
    }
}

/// What a stereo file's two channels hold relative to one another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StereoContent {
    /// Not a two-channel file, or the encoder gave nothing away.
    Unknown,
    /// The channels differ: real stereo.
    Independent,
    /// Both channels are identical: mono in a stereo container.
    Duplicated,
}

/// Walks the frames of a FLAC file and reports what they reveal.
pub fn audit(path: &Path, limits: Limits) -> Result<FlacAudit, TagError> {
    let mut file = crate::tags::bytes::open_regular(path)?;
    let (stream_info, audio_start) = read_stream_info(&mut file)?;

    let budget = match limits.max_bytes {
        usize::MAX => file.metadata()?.len().saturating_sub(audio_start) as usize,
        bytes => bytes,
    };
    let data = read_at_most(&mut file, audio_start, budget)?;
    let reached_end_of_file = audio_start + data.len() as u64 >= file.metadata()?.len();
    let mut reader = BitReader::new(&data);

    let mut min_wasted = u32::MAX;
    let mut confirmed_wasted = u32::MAX;
    let mut precision = precision::PrecisionConfirmation::new(stream_info.raw);
    let mut frames = 0usize;
    let mut all_constant = true;
    let mut duplicated_frames = 0usize;
    let mut stereo_frames = 0usize;

    while let Some(frame) = read_frame(&mut reader, &stream_info) {
        frames += 1;
        min_wasted = min_wasted.min(frame.min_wasted);
        // Nonzero constant frames have no structural minimum of their own,
        // but their significant bits can refute a later padding candidate.
        // Digital zero is neutral. Once a sample refutes padding, no more
        // decoding is needed for this file.
        if min_wasted > 0 && confirmed_wasted > 0 && !frame.all_constant {
            let confirmed = data
                .get(frame.span.start..frame.span.end)
                .and_then(|bytes| precision.confirm(bytes, frame.block_size))
                .unwrap_or(0);
            confirmed_wasted = confirmed_wasted.min(confirmed);
        }
        all_constant &= frame.all_constant;
        if frame.is_stereo {
            stereo_frames += 1;
            if frame.channels_identical {
                duplicated_frames += 1;
            }
        }
        // Frames are byte aligned; the next sync code starts on a boundary.
        reader.align_to_byte();
        if reader.is_exhausted() || frames >= limits.max_frames {
            break;
        }
    }

    let wasted = if frames == 0 || min_wasted == u32::MAX {
        0
    } else {
        min_wasted.min(confirmed_wasted)
    };
    let effective = (stream_info.bits_per_sample as u32).saturating_sub(wasted);

    let stereo = if stream_info.channels != 2 || stereo_frames == 0 {
        StereoContent::Unknown
    } else if duplicated_frames == stereo_frames {
        StereoContent::Duplicated
    } else {
        StereoContent::Independent
    };

    Ok(FlacAudit {
        declared_bit_depth: stream_info.bits_per_sample,
        effective_bit_depth: effective.max(1) as u16,
        channels: stream_info.channels,
        stereo,
        digital_silence: frames > 0 && all_constant,
        frames_examined: frames,
        truncated: !reached_end_of_file || frames >= limits.max_frames,
    })
}

/// The few STREAMINFO fields the frame walk needs.
pub(crate) struct StreamInfo {
    pub(crate) bits_per_sample: u16,
    pub(crate) channels: u16,
    // Older structural readers accepted the first 18 bytes. Missing decoder
    // metadata must not change their checksum behavior or support a claim.
    raw: Option<[u8; 34]>,
}

/// Reads STREAMINFO and returns it with the offset of the first frame.
pub(crate) fn read_stream_info(file: &mut std::fs::File) -> Result<(StreamInfo, u64), TagError> {
    let start = crate::tags::id3::skip_id3v2(file)?;
    let header = read_at_most(file, start, 4)?;
    if header.len() < 4 || &header[..4] != b"fLaC" {
        return Err(TagError::UnrecognizedFormat);
    }

    let mut offset = start + 4;
    let mut info = None;
    loop {
        let head = read_at_most(file, offset, 4)?;
        if head.len() < 4 {
            return Err(TagError::Malformed("metadata block header truncated"));
        }
        let last = head[0] & 0x80 != 0;
        let kind = head[0] & 0x7F;
        let length = ((head[1] as u64) << 16) | ((head[2] as u64) << 8) | head[3] as u64;
        offset += 4;

        if kind == 0 {
            let body = read_at_most(file, offset, length.min(34) as usize)?;
            if body.len() < 18 {
                return Err(TagError::Malformed("STREAMINFO truncated"));
            }
            let packed = u64::from_be_bytes([
                body[10], body[11], body[12], body[13], body[14], body[15], body[16], body[17],
            ]);
            info = Some(StreamInfo {
                channels: (((packed >> 41) & 0x07) as u16) + 1,
                bits_per_sample: (((packed >> 36) & 0x1F) as u16) + 1,
                raw: body.as_slice().try_into().ok(),
            });
        }
        offset += length;
        if last {
            break;
        }
    }

    match info {
        Some(info) => Ok((info, offset)),
        None => Err(TagError::Malformed("FLAC stream without STREAMINFO")),
    }
}

/// What one frame contributes to the audit.
pub(crate) struct FrameAudit {
    min_wasted: u32,
    block_size: u32,
    all_constant: bool,
    is_stereo: bool,
    channels_identical: bool,
    /// Byte range of the frame within the slice being walked, and where its
    /// header stops. The checksum walk and precision confirmation share these
    /// already-read bytes.
    pub(crate) span: FrameSpan,
}

/// Where a frame sits in the stream, and the checksums it carries.
pub(crate) struct FrameSpan {
    /// Offset of the sync code.
    pub(crate) start: usize,
    /// Offset just past the frame's own CRC-16.
    pub(crate) end: usize,
    /// Offset of the header's CRC-8, which is also where the header stops.
    pub(crate) header_end: usize,
    /// The CRC-8 the header announces.
    pub(crate) stored_crc8: u8,
    /// The CRC-16 the frame announces.
    pub(crate) stored_crc16: u16,
}

/// Reads one frame header and walks its subframes.
///
/// Returns `None` at the end of the stream or on anything unexpected, which
/// ends the walk without failing the audit.
pub(crate) fn read_frame(reader: &mut BitReader<'_>, info: &StreamInfo) -> Option<FrameAudit> {
    let start = reader.bit_pos() / 8;
    if reader.bits(14)? != 0b11_1111_1111_1110 {
        return None;
    }
    reader.bits(1)?; // reserved
    reader.bits(1)?; // blocking strategy

    let block_size_bits = reader.bits(4)? as u32;
    let sample_rate_bits = reader.bits(4)? as u32;
    let channel_assignment = reader.bits(4)? as u32;
    let sample_size_bits = reader.bits(3)? as u32;
    reader.bits(1)?; // reserved

    // Coded frame or sample number, in a UTF-8-like variable length encoding.
    let first = reader.bits(8)? as u8;
    let extra = match first {
        0x00..=0x7F => 0,
        0xC0..=0xDF => 1,
        0xE0..=0xEF => 2,
        0xF0..=0xF7 => 3,
        0xF8..=0xFB => 4,
        0xFC..=0xFD => 5,
        0xFE => 6,
        _ => return None,
    };
    reader.skip(extra * 8)?;

    let block_size = match block_size_bits {
        0 => return None,
        1 => 192,
        2..=5 => 576 << (block_size_bits - 2),
        6 => reader.bits(8)? as u32 + 1,
        7 => reader.bits(16)? as u32 + 1,
        _ => 256 << (block_size_bits - 8),
    };
    match sample_rate_bits {
        12 => {
            reader.bits(8)?;
        }
        13 | 14 => {
            reader.bits(16)?;
        }
        15 => return None,
        _ => {}
    }
    let header_end = reader.bit_pos() / 8;
    let stored_crc8 = reader.bits(8)? as u8;

    let bits_per_sample = match sample_size_bits {
        0 => info.bits_per_sample as u32,
        1 => 8,
        2 => 12,
        4 => 16,
        5 => 20,
        6 => 24,
        7 => 32,
        _ => return None,
    };

    // Channel assignments 8, 9 and 10 store a difference channel, which needs
    // one extra bit and tells us whether the channels are identical.
    let (channel_count, side_index) = match channel_assignment {
        0..=7 => (channel_assignment + 1, None),
        8 => (2, Some(1)),
        9 => (2, Some(0)),
        10 => (2, Some(1)),
        _ => return None,
    };
    // Mid/side stores (left + right) >> 1. Restoring its candidate bit retains
    // detection of padded streams; it is not proof about the original low
    // bits. The public precision audit confirms any positive candidate.
    let mid_index = if channel_assignment == 10 {
        Some(0)
    } else {
        None
    };

    let mut min_wasted = u32::MAX;
    let mut all_constant = true;
    let mut side_is_zero = false;

    for channel in 0..channel_count {
        let extra_bit = side_index == Some(channel);
        let bps = bits_per_sample + if extra_bit { 1 } else { 0 };
        let sub = read_subframe(reader, block_size, bps)?;
        // Constant headers alone cannot establish precision: the encoder has
        // no reason to record wasted bits on a value stored once. Ignore them
        // for this candidate, then confirm their actual samples in the audit.
        if sub.constant_value.is_none() {
            let corrected = sub.wasted + if mid_index == Some(channel) { 1 } else { 0 };
            min_wasted = min_wasted.min(corrected);
        }
        all_constant &= sub.constant_value == Some(0);
        if extra_bit && sub.constant_value == Some(0) {
            side_is_zero = true;
        }
    }

    reader.align_to_byte();
    let stored_crc16 = reader.bits(16)? as u16;

    Some(FrameAudit {
        min_wasted,
        block_size,
        all_constant,
        is_stereo: side_index.is_some(),
        channels_identical: side_is_zero,
        span: FrameSpan {
            start,
            end: reader.bit_pos() / 8,
            header_end,
            stored_crc8,
            stored_crc16,
        },
    })
}

/// What one subframe contributes.
struct SubframeAudit {
    wasted: u32,
    /// The value, when the subframe is a constant one.
    constant_value: Option<i64>,
}

/// Reads a subframe header and steps over its data.
///
/// The residual is parsed but not reconstructed: Rice codes are
/// variable-length, so they must be read to be skipped, but nothing needs to
/// be computed from them.
fn read_subframe(
    reader: &mut BitReader<'_>,
    block_size: u32,
    bits_per_sample: u32,
) -> Option<SubframeAudit> {
    if reader.bits(1)? != 0 {
        return None; // padding bit must be zero
    }
    let kind = reader.bits(6)? as u32;
    let has_wasted = reader.bits(1)? == 1;
    let wasted = if has_wasted { reader.unary()? + 1 } else { 0 };

    let bps = bits_per_sample.checked_sub(wasted)?;
    if bps == 0 || bps > 33 {
        return None;
    }

    let mut constant_value = None;
    match kind {
        0 => {
            constant_value = Some(reader.signed_bits(bps)?);
        }
        1 => {
            reader.skip(block_size as usize * bps as usize)?;
        }
        8..=12 => {
            let order = kind - 8;
            reader.skip(order as usize * bps as usize)?;
            skip_residual(reader, block_size, order)?;
        }
        32..=63 => {
            let order = kind - 31;
            reader.skip(order as usize * bps as usize)?;
            let precision = reader.bits(4)? as u32 + 1;
            if precision > 15 {
                return None; // 0b1111 is invalid
            }
            reader.bits(5)?; // quantisation shift
            reader.skip(order as usize * precision as usize)?;
            skip_residual(reader, block_size, order)?;
        }
        _ => return None, // reserved
    }

    Some(SubframeAudit {
        wasted,
        constant_value,
    })
}

/// Steps over a residual, partition by partition.
fn skip_residual(reader: &mut BitReader<'_>, block_size: u32, order: u32) -> Option<()> {
    let method = reader.bits(2)?;
    let param_bits = match method {
        0 => 4,
        1 => 5,
        _ => return None, // reserved
    };
    let escape = (1u64 << param_bits) - 1;

    let partition_order = reader.bits(4)? as u32;
    let partitions = 1u32 << partition_order;
    if !block_size.is_multiple_of(partitions) {
        return None;
    }

    for partition in 0..partitions {
        let mut samples = block_size >> partition_order;
        if partition == 0 {
            samples = samples.checked_sub(order)?;
        }
        let parameter = reader.bits(param_bits)?;
        if parameter == escape {
            let raw_bits = reader.bits(5)? as usize;
            reader.skip(samples as usize * raw_bits)?;
        } else {
            for _ in 0..samples {
                reader.unary()?;
                reader.bits(parameter as u32)?;
            }
        }
    }
    Some(())
}
