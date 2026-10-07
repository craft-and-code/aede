//! Progressive source-depth integers for the future transparent playback path.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use flaccompagnon_core::AnalysisError;

use crate::playback::format::IntegerPcmFormat;

use super::flac::IntegerFlacStream;
use super::integer_wav::IntegerWavStream;
use super::{Error, FlacMd5Status, SeekResult};

/// Decode native FLAC or PCM WAV without converting the source to floating PCM.
///
/// The initial profile accepts integer 16/24-bit mono/stereo sources. Values
/// returned in `i32` containers retain their source-depth scale, interleaved
/// channel order and rate. This is a source API, not an audio sink or a claim
/// about operating-system/device output. No DSP, FFmpeg or fallback is used.
pub struct IntegerFileDecoder {
    source: Source,
    format: IntegerPcmFormat,
    pending: Vec<i32>,
    pending_offset: usize,
    finished: bool,
}

enum Source {
    Flac(IntegerFlacStream),
    Wav(IntegerWavStream),
}

impl IntegerFileDecoder {
    /// Inspect and decode the same opened regular file; extensions are not trusted.
    pub fn open(path: &Path) -> Result<Self, Error> {
        // Reuse the source preflight so a FIFO/device is refused before a
        // potentially blocking open. Recheck the actual descriptor below.
        crate::copy::validate_source(path).map_err(decode_error)?;
        let mut file = File::open(path).map_err(decode_error)?;
        if !file.metadata().map_err(decode_error)?.is_file() {
            return Err(Error::UnsupportedIntegerSource(
                "integer playback requires a local regular file",
            ));
        }
        let mut magic = [0; 4];
        file.read_exact(&mut magic).map_err(decode_error)?;
        file.seek(SeekFrom::Start(0)).map_err(decode_error)?;
        let (source, format) = if &magic == b"RIFF" {
            let wav = IntegerWavStream::open(file)?;
            let format = IntegerPcmFormat::new(
                wav.sample_rate(),
                wav.channel_layout(),
                wav.bits_per_sample(),
            )
            .map_err(|_| Error::InvalidFormat)?;
            if format.channels() != wav.channels() {
                return Err(Error::InvalidFormat);
            }
            (Source::Wav(wav), format)
        } else if &magic == b"fLaC" || &magic[..3] == b"ID3" {
            let flac = IntegerFlacStream::open_integer(file)?;
            let format =
                IntegerPcmFormat::new(flac.sample_rate, flac.layout, flac.bits_per_sample())
                    .map_err(|_| Error::InvalidFormat)?;
            (Source::Flac(flac), format)
        } else {
            return Err(Error::UnsupportedIntegerSource(
                "integer playback requires native 16/24-bit mono/stereo FLAC or PCM WAV",
            ));
        };
        Ok(Self {
            source,
            format,
            pending: Vec::new(),
            pending_offset: 0,
            finished: false,
        })
    }

    /// Original rate, valid precision and channel association.
    pub fn format(&self) -> IntegerPcmFormat {
        self.format
    }

    /// FLAC verification status for this decode pass, finalized only at EOF.
    pub fn flac_md5_status(&self) -> Option<FlacMd5Status> {
        match &self.source {
            Source::Flac(flac) => Some(flac.md5_status()),
            Source::Wav(_) => None,
        }
    }

    /// Read complete interleaved frames into a caller-owned integer buffer.
    ///
    /// A short read is normal; only `Ok(0)` establishes successful EOF. Empty
    /// or non-frame-aligned buffers are rejected without advancing the source
    /// or mutating the buffer. Unused output elements stay unchanged.
    pub fn read_frames(&mut self, output: &mut [i32]) -> Result<usize, Error> {
        let channels = usize::from(self.format.channels());
        if output.is_empty() || !output.len().is_multiple_of(channels) {
            return Err(Error::InvalidBuffer);
        }
        if !self.fill_pending()? {
            return Ok(0);
        }
        let count = output.len().min(self.pending.len() - self.pending_offset);
        output[..count]
            .copy_from_slice(&self.pending[self.pending_offset..self.pending_offset + count]);
        self.pending_offset += count;
        Ok(count / channels)
    }

    /// Progressively discard source frames for a later explicitly requested seek.
    ///
    /// No output, DSP or listening history is produced. FLAC verification
    /// includes the discarded prefix. Working memory stays bounded, but work
    /// grows with the traversed audio. Cancellation is polled before decoding
    /// and after at most 4096 discarded frames; a decoder/I/O call may block.
    /// Discard this decoder after cancellation or failure rather than treating
    /// a partially advanced position as a successful seek.
    pub fn skip_frames(
        &mut self,
        frames: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SeekResult, Error> {
        let channels = usize::from(self.format.channels());
        let mut skipped = 0;
        loop {
            if cancelled() {
                return Err(Error::SeekCancelled);
            }
            if skipped == frames {
                return Ok(SeekResult {
                    frames: skipped,
                    reached_eof: false,
                });
            }
            if !self.fill_pending()? {
                return Ok(SeekResult {
                    frames: skipped,
                    reached_eof: true,
                });
            }
            let available = (self.pending.len() - self.pending_offset) / channels;
            let count = available.min((frames - skipped).min(4096) as usize);
            self.pending_offset += count * channels;
            skipped += count as u64;
        }
    }

    fn fill_pending(&mut self) -> Result<bool, Error> {
        if self.finished {
            return Ok(false);
        }
        while self.pending_offset == self.pending.len() {
            let chunk = match &mut self.source {
                Source::Flac(flac) => {
                    let chunk = flac.next_chunk()?;
                    // Symphonia's S32 FLAC samples are already left-aligned;
                    // undo that alignment once, keeping the original low bits.
                    chunk.map(|mut samples| {
                        let shift = 32 - self.format.bits_per_sample();
                        for sample in &mut samples {
                            *sample >>= shift;
                        }
                        samples
                    })
                }
                Source::Wav(wav) => wav.next_chunk()?,
            };
            let Some(samples) = chunk else {
                self.finished = true;
                return Ok(false);
            };
            self.pending = samples;
            self.pending_offset = 0;
        }
        Ok(true)
    }
}

fn decode_error(error: impl std::fmt::Display) -> Error {
    Error::Decode(AnalysisError::Decode(error.to_string()))
}

#[cfg(test)]
#[path = "integer_tests.rs"]
mod tests;
