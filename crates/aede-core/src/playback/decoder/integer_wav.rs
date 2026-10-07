//! Bounded RIFF/WAVE admission and progressive, unscaled integer PCM reads.
//!
//! This adapter deliberately does not reuse the float-only preview decoder.
//! Every returned `i32` retains the source's signed 16- or 24-bit sample value;
//! widening for an output container belongs to the later output adapter.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use aede_dsp::ChannelLayout;
use flaccompagnon_core::AnalysisError;

use super::Error;

const MAX_CHUNKS: usize = 65_536;
const MAX_CHUNK_FRAMES: usize = 4096;
const PCM_SUBTYPE: [u8; 16] = [
    1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
];

pub(super) struct IntegerWavStream {
    file: File,
    format: PcmFormat,
    remaining: u64,
    bytes: Vec<u8>,
    failure: Option<String>,
}

struct PcmFormat {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u32,
    block_align: u16,
    layout: ChannelLayout,
}

impl IntegerWavStream {
    /// Inspect all chunk bounds on this regular descriptor without loading PCM.
    ///
    /// The initial candidate profile deliberately requires an exact RIFF/file
    /// extent, one data chunk and a 16/18-byte PCM or 40-byte extensible format.
    /// Other legal WAVE variants remain outside this adapter's exact contract.
    pub(super) fn open(mut file: File) -> Result<Self, Error> {
        let metadata = file.metadata().map_err(decode_error)?;
        if !metadata.is_file() {
            return Err(decode_error("WAV source is not a regular file"));
        }
        let file_length = metadata.len();
        file.seek(SeekFrom::Start(0)).map_err(decode_error)?;
        let mut header = [0; 12];
        file.read_exact(&mut header).map_err(decode_error)?;
        if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
            return Err(Error::InvalidFormat);
        }
        let riff_end = u64::from(u32_at(&header, 4)) + 8;
        // A strict source cannot hide undeclared audio beyond its RIFF extent.
        if riff_end != file_length || riff_end < 12 {
            return Err(decode_error(
                "WAV RIFF size disagrees with the source length",
            ));
        }
        let mut format = None;
        let mut data = None;
        let mut offset = 12_u64;
        let mut chunks = 0;
        while offset < riff_end {
            if chunks == MAX_CHUNKS {
                return Err(decode_error("WAV exceeds the chunk-count limit"));
            }
            chunks += 1;
            let body = offset
                .checked_add(8)
                .filter(|&end| end <= riff_end)
                .ok_or_else(|| decode_error("WAV chunk header is truncated"))?;
            file.seek(SeekFrom::Start(offset)).map_err(decode_error)?;
            let mut chunk = [0; 8];
            file.read_exact(&mut chunk).map_err(decode_error)?;
            let size = u64::from(u32_at(&chunk, 4));
            let padded_end = body
                .checked_add(size)
                .and_then(|end| end.checked_add(size & 1))
                .filter(|&end| end <= riff_end)
                .ok_or_else(|| decode_error("WAV chunk exceeds its RIFF extent"))?;
            match &chunk[..4] {
                b"fmt " => {
                    if format.is_some() {
                        return Err(decode_error("WAV has duplicate format chunks"));
                    }
                    format = Some(read_format(&mut file, size)?);
                }
                b"data" => {
                    if data.is_some() {
                        return Err(decode_error("WAV has duplicate audio data chunks"));
                    }
                    data = Some((body, size));
                }
                b"LIST" if size >= 4 => {
                    let mut kind = [0; 4];
                    file.read_exact(&mut kind).map_err(decode_error)?;
                    // A wavl list is another waveform sequence, not metadata.
                    if &kind == b"wavl" {
                        return Err(decode_error("WAV waveform lists are unsupported"));
                    }
                }
                // Artwork and other non-audio metadata remain untouched.
                _ => {}
            }
            offset = padded_end;
        }
        let format = format.ok_or_else(|| decode_error("WAV has no format chunk"))?;
        let (data_offset, remaining) =
            data.ok_or_else(|| decode_error("WAV has no audio data chunk"))?;
        if !remaining.is_multiple_of(u64::from(format.block_align)) {
            return Err(Error::IncompleteFrame);
        }
        file.seek(SeekFrom::Start(data_offset))
            .map_err(decode_error)?;
        Ok(Self {
            file,
            format,
            remaining,
            bytes: Vec::new(),
            failure: None,
        })
    }

    pub(super) fn sample_rate(&self) -> u32 {
        self.format.sample_rate
    }

    pub(super) fn channels(&self) -> u16 {
        self.format.channels
    }

    pub(super) fn bits_per_sample(&self) -> u32 {
        self.format.bits_per_sample
    }

    pub(super) fn channel_layout(&self) -> ChannelLayout {
        self.format.layout
    }

    /// Return at most 4096 complete frames, with no scaling or DSP.
    ///
    /// A failed read is terminal even when the file is repaired afterwards.
    pub(super) fn next_chunk(&mut self) -> Result<Option<Vec<i32>>, Error> {
        if let Some(failure) = &self.failure {
            return Err(decode_error(failure));
        }
        if self.remaining == 0 {
            return Ok(None);
        }
        let maximum = MAX_CHUNK_FRAMES * usize::from(self.format.block_align);
        let count = usize::try_from(self.remaining.min(maximum as u64))
            .map_err(|_| Error::InvalidFormat)?;
        self.bytes.resize(count, 0);
        if let Err(error) = self.file.read_exact(&mut self.bytes) {
            let failure = format!("WAV audio data could not be read: {error}");
            self.failure = Some(failure.clone());
            return Err(decode_error(failure));
        }
        self.remaining -= count as u64;
        let samples = if self.format.bits_per_sample == 16 {
            self.bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&sample| i32::from(i16::from_le_bytes(sample)))
                .collect()
        } else {
            self.bytes
                .as_chunks::<3>()
                .0
                .iter()
                .map(|sample| {
                    let value = i32::from_le_bytes([sample[0], sample[1], sample[2], 0]);
                    (value << 8) >> 8
                })
                .collect()
        };
        Ok(Some(samples))
    }
}

fn read_format(file: &mut File, size: u64) -> Result<PcmFormat, Error> {
    if !matches!(size, 16 | 18 | 40) {
        return Err(Error::InvalidFormat);
    }
    let mut bytes = [0; 40];
    file.read_exact(&mut bytes[..size as usize])
        .map_err(decode_error)?;
    let tag = u16_at(&bytes, 0);
    let channels = u16_at(&bytes, 2);
    let sample_rate = u32_at(&bytes, 4);
    let byte_rate = u32_at(&bytes, 8);
    let block_align = u16_at(&bytes, 12);
    let bits_per_sample = u16_at(&bytes, 14);
    if !matches!(channels, 1 | 2)
        || !matches!(bits_per_sample, 16 | 24)
        || sample_rate == 0
        || block_align != channels * (bits_per_sample / 8)
        || sample_rate.checked_mul(u32::from(block_align)) != Some(byte_rate)
    {
        return Err(Error::InvalidFormat);
    }
    let layout = match (tag, size) {
        (1, 16) => canonical_layout(channels),
        (1, 18) if u16_at(&bytes, 16) == 0 => canonical_layout(channels),
        (0xfffe, 40)
            if u16_at(&bytes, 16) == 22
                && u16_at(&bytes, 18) == bits_per_sample
                && bytes[24..40] == PCM_SUBTYPE =>
        {
            match (channels, u32_at(&bytes, 20)) {
                (1, 0 | 0x4) => ChannelLayout::MONO,
                (2, 0x3) => ChannelLayout::STEREO,
                _ => return Err(Error::ChannelLayout("unsupported WAV speaker mask".into())),
            }
        }
        _ => return Err(Error::InvalidFormat),
    };
    Ok(PcmFormat {
        sample_rate,
        channels,
        bits_per_sample: u32::from(bits_per_sample),
        block_align,
        layout,
    })
}

fn canonical_layout(channels: u16) -> ChannelLayout {
    if channels == 1 {
        ChannelLayout::MONO
    } else {
        ChannelLayout::STEREO
    }
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn decode_error(error: impl std::fmt::Display) -> Error {
    Error::Decode(AnalysisError::Decode(error.to_string()))
}

#[cfg(test)]
#[path = "integer_wav_tests.rs"]
mod tests;
