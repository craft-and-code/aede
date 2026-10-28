//! Retained-file DSD bits, independent of filtering and device encapsulation.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use super::{DsdError, Info, container, source_length};

/// Bounded original bits, interleaved by byte and channel, oldest bit first in
/// the least-significant position. High unused bits in the last byte are zero.
pub struct RawBlock {
    /// Original one-bit position of this block, per channel.
    pub first_sample: u64,
    /// Valid original one-bit samples per channel, excluding container padding.
    pub valid_samples: u64,
    /// Ordered source channels; no mixing or duplication is performed.
    pub channels: usize,
    /// Interleaved canonical DSD bytes. This is never PCM audio.
    pub bytes: Vec<u8>,
}

pub(super) struct ByteFrame {
    pub bytes: [u8; 2],
    pub valid_bits: u8,
}

/// Progressive uncompressed DSF/DFF mono/stereo source with no filtering.
/// Container inspection does not establish any device's DSD/DoP capability.
/// Seeking retains the same opened file even if its pathname is replaced.
pub struct RawSource {
    file: File,
    layout: container::Layout,
    buffer: Vec<u8>,
    buffer_start: u64,
    buffered_frames: usize,
    position: u64,
    failed: bool,
}

impl RawSource {
    /// Open a regular read-only file; select its container by signature.
    pub fn open(path: &Path) -> Result<Self, DsdError> {
        Self::from_file(File::open(path)?)
    }

    /// Parse and retain this descriptor without reading tags or decoding PCM.
    pub fn from_file(mut file: File) -> Result<Self, DsdError> {
        let length = source_length(&file)?;
        let layout = container::inspect(&mut file, length)?;
        if !matches!(layout.info.channels, 1 | 2) {
            return Err(DsdError(
                "raw DSD requires known mono/stereo channels".into(),
            ));
        }
        Ok(Self {
            buffer: vec![0; 4096 * layout.info.channels],
            file,
            layout,
            buffer_start: 0,
            buffered_frames: 0,
            position: 0,
            failed: false,
        })
    }

    /// Source clock and exact original length, without device format hints.
    pub fn info(&self) -> Info {
        self.layout.info
    }

    /// Next original one-bit position, saturated at the declared source end.
    pub fn position(&self) -> u64 {
        self.position.saturating_mul(8).min(self.info().samples)
    }

    /// Move to an eight-bit boundary or the exact source end. Invalid requests
    /// leave the current position intact; a retired source cannot be revived.
    pub fn seek(&mut self, sample: u64) -> Result<(), DsdError> {
        self.live()?;
        if sample > self.info().samples
            || (!sample.is_multiple_of(8) && sample != self.info().samples)
        {
            return Err(DsdError(
                "DSD seek requires a byte boundary within the source".into(),
            ));
        }
        self.position = sample.div_ceil(8);
        Ok(())
    }

    /// Return at most 4,096 complete channel-byte frames, checking admission
    /// before/after I/O and before publication. Any read/admission error retires
    /// the source permanently; an in-flight filesystem call is not interrupted.
    pub fn next_chunk(
        &mut self,
        mut current: impl FnMut() -> Result<(), DsdError>,
    ) -> Result<Option<RawBlock>, DsdError> {
        self.live()?;
        let result = self.chunk(&mut current);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn chunk(
        &mut self,
        current: &mut impl FnMut() -> Result<(), DsdError>,
    ) -> Result<Option<RawBlock>, DsdError> {
        current()?;
        let first_sample = self.position();
        if first_sample == self.info().samples {
            return Ok(None);
        }
        let channels = self.info().channels;
        let mut bytes = Vec::with_capacity(4096 * channels);
        let mut valid_samples = 0;
        for _ in 0..4096 {
            current()?;
            let frame = self.next_byte_frame()?;
            current()?;
            let Some(frame) = frame else {
                break;
            };
            bytes.extend_from_slice(&frame.bytes[..channels]);
            valid_samples += u64::from(frame.valid_bits);
        }
        current()?;
        Ok(Some(RawBlock {
            first_sample,
            valid_samples,
            channels,
            bytes,
        }))
    }

    fn live(&self) -> Result<(), DsdError> {
        if self.failed {
            Err(DsdError("DSD bit source is retired".into()))
        } else {
            Ok(())
        }
    }

    pub(super) fn next_byte_frame(&mut self) -> Result<Option<ByteFrame>, DsdError> {
        self.live()?;
        let result = self.byte_frame();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn byte_frame(&mut self) -> Result<Option<ByteFrame>, DsdError> {
        let count = self.info().samples.div_ceil(8);
        if self.position == count {
            return Ok(None);
        }
        if self.buffered_frames == 0
            || self.position < self.buffer_start
            || self.position - self.buffer_start >= self.buffered_frames as u64
        {
            self.refill(count)?;
        }
        let at = (self.position - self.buffer_start) as usize;
        let valid_bits = if self.position + 1 == count && !self.info().samples.is_multiple_of(8) {
            (self.info().samples % 8) as u8
        } else {
            8
        };
        let mask = ((1u16 << valid_bits) - 1) as u8;
        let mut frame = ByteFrame {
            bytes: [0; 2],
            valid_bits,
        };
        for channel in 0..self.info().channels {
            let index = if self.layout.block == 1 {
                at * self.info().channels + channel
            } else {
                channel * self.layout.block + at
            };
            let byte = self.buffer[index];
            frame.bytes[channel] = (if self.layout.lsb {
                byte
            } else {
                byte.reverse_bits()
            }) & mask;
        }
        self.position += 1;
        Ok(Some(frame))
    }

    fn refill(&mut self, count: u64) -> Result<(), DsdError> {
        let channels = self.info().channels;
        let (start, frames, bytes) = if self.layout.block == 1 {
            let frames = (count - self.position).min(4096) as usize;
            (self.position, frames, frames * channels)
        } else {
            let start = self.position / 4096 * 4096;
            (start, (count - start).min(4096) as usize, 4096 * channels)
        };
        let relative = start
            .checked_mul(channels as u64)
            .and_then(|at| self.layout.offset.checked_add(at))
            .ok_or_else(|| DsdError("DSD byte offset overflow".into()))?;
        self.file.seek(SeekFrom::Start(relative))?;
        self.file.read_exact(&mut self.buffer[..bytes])?;
        self.buffer_start = start;
        self.buffered_frames = frames;
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
