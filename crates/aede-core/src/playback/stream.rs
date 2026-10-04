//! Reusable decoded PCM blocks for local and server playback drivers.
//!
//! This layer owns decoding and `f32le` encoding. The caller supplies optional
//! sample processing, an output sink, and any transport or progress controls.
//! It never opens a device or sends data over a network.

use std::error::Error;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use super::decoder::{self, FileDecoder};
use super::format::PcmStreamFormat;
use aede_dsp::{DspError, RateConverter, RateError, StereoDownmixer};

const BLOCK_FRAMES: usize = 4096;

/// A decoded block after caller-supplied processing.
pub struct PcmBlock<'a> {
    /// Interleaved floating-point samples for meters and visualization.
    pub samples: &'a [f32],
    /// The same samples encoded as little-endian `f32` for an output sink.
    pub f32le: &'a [u8],
    /// Complete PCM frames in the block.
    pub frames: usize,
}

/// A failure while decoding or processing a PCM block.
#[derive(Debug)]
pub enum StreamError<E> {
    /// The source file could not supply valid PCM.
    Decode(decoder::Error),
    /// The source channel layout or a PCM frame cannot be downmixed.
    Downmix(DspError),
    /// A requested output rate could not be produced.
    Resample(RateError),
    /// The caller's DSP stage rejected the block.
    Process(E),
    /// The caller's stage produced a non-finite output sample.
    NonFiniteSample,
}

impl<E: fmt::Display> fmt::Display for StreamError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "{error}"),
            Self::Downmix(error) => write!(f, "stereo downmix failed: {error}"),
            Self::Resample(error) => write!(f, "{error}"),
            Self::Process(error) => write!(f, "audio processing failed: {error}"),
            Self::NonFiniteSample => f.write_str("audio processing produced a non-finite sample"),
        }
    }
}

impl<E: Error + 'static> Error for StreamError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Downmix(error) => Some(error),
            Self::Resample(error) => Some(error),
            Self::Process(error) => Some(error),
            Self::NonFiniteSample => None,
        }
    }
}

/// A progressive PCM stream with one fixed source format.
pub struct PcmTrack {
    decoder: FileDecoder,
    format: PcmStreamFormat,
    source_format: PcmStreamFormat,
    decode_format: PcmStreamFormat,
    downmix: Option<StereoDownmixer>,
    resampler: Option<RateConverter>,
    started: bool,
    ended: bool,
    samples: Vec<f32>,
    stereo: Vec<f32>,
    converted: Vec<f32>,
    bytes: Vec<u8>,
}

impl PcmTrack {
    /// Open a local audio file without decoding it in full.
    pub fn open(path: &Path) -> Result<Self, decoder::Error> {
        let decoder = FileDecoder::open(path)?;
        let format = PcmStreamFormat::with_layout(decoder.sample_rate(), decoder.channel_layout())
            .map_err(|_| decoder::Error::InvalidFormat)?;
        let sample_capacity = BLOCK_FRAMES * usize::from(format.channels());
        Ok(Self {
            decoder,
            format,
            source_format: format,
            decode_format: format,
            downmix: None,
            resampler: None,
            started: false,
            ended: false,
            samples: vec![0.0; sample_capacity],
            stereo: Vec::new(),
            converted: Vec::new(),
            bytes: vec![0; sample_capacity * 4],
        })
    }

    /// Prepare stereo output for known multichannel material. Mono and
    /// stereo keep their original samples and channel positions.
    pub fn open_stereo(path: &Path) -> Result<Self, Box<dyn Error>> {
        let mut track = Self::open(path)?;
        if track.format.channels() > 2 {
            let downmix = StereoDownmixer::new(track.format.layout())?;
            track.format = PcmStreamFormat::new(track.format.sample_rate(), 2)?;
            track.decode_format = track.format;
            track.stereo = vec![0.0; BLOCK_FRAMES * 2];
            track.bytes = vec![0; BLOCK_FRAMES * 2 * 4];
            track.downmix = Some(downmix);
        }
        Ok(track)
    }

    /// The PCM format a sink must accept for this track.
    pub fn format(&self) -> PcmStreamFormat {
        self.format
    }

    /// Decoded source format before an optional stereo downmix.
    pub fn source_format(&self) -> PcmStreamFormat {
        self.source_format
    }

    /// Choose the sink's sample rate before seeking or reading the first block. When
    /// rates match, samples retain the exact decoded path.
    pub fn set_output_rate(&mut self, rate: u32) -> Result<(), RateError> {
        if self.started || self.resampler.is_some() {
            return Err(RateError::Finished);
        }
        if rate != self.decode_format.sample_rate() {
            let converter = RateConverter::new(self.decode_format, rate)?;
            self.format = PcmStreamFormat::with_layout(rate, self.decode_format.layout())
                .map_err(|_| RateError::InvalidFormat)?;
            self.resampler = Some(converter);
        }
        Ok(())
    }

    /// Position a newly opened track at the source frame at or before `position`.
    ///
    /// Seeking progressively decodes and discards the prefix, without sending
    /// it through the source observer, downmix, resampler or caller's DSP. It
    /// preserves the ordinary decoder's delay, padding and error semantics and
    /// uses no whole-file buffer. Work grows with the requested position;
    /// cancellation is checked between bounded frame steps, though one decoder
    /// call may block on I/O. A position beyond EOF returns the actual frame
    /// count with `reached_eof` set, rather than trusting tag duration.
    ///
    /// Call at most once, before reading PCM. Set the output rate first when
    /// needed. A backward or subsequent seek requires reopening the track and
    /// resetting the driver's output and DSP session. After cancellation or a
    /// decode error, discard this track. Measurements of the suffix are not a
    /// full-track loudness result and must not be cached as one.
    pub fn seek_from_start(
        &mut self,
        position: Duration,
        cancelled: impl FnMut() -> bool,
    ) -> Result<decoder::SeekResult, decoder::Error> {
        if self.started {
            return Err(decoder::Error::SeekAfterStart);
        }
        let rate = u128::from(self.source_format.sample_rate());
        let frames = u128::from(position.as_secs()) * rate
            + u128::from(position.subsec_nanos()) * rate / 1_000_000_000;
        let frames = u64::try_from(frames).map_err(|_| decoder::Error::InvalidPosition)?;
        self.started = true;
        let result = self.decoder.skip_frames(frames, cancelled)?;
        self.ended = result.reached_eof;
        Ok(result)
    }

    /// Read, optionally process, and encode the next block.
    ///
    /// Returns `None` only at end of file. The processing function is called
    /// once for each nonempty block and may retain state between calls. The
    /// returned views are valid until the next call to `read_block`.
    pub fn read_block<E>(
        &mut self,
        process: impl FnOnce(&mut [f32]) -> Result<(), E>,
    ) -> Result<Option<PcmBlock<'_>>, StreamError<E>> {
        self.read_block_observed(|_, _| {}, process)
    }

    /// Read a block while observing decoded source PCM before any downmix,
    /// rate conversion or caller-supplied processing.
    ///
    /// The observer receives each nonempty source block exactly once with its
    /// original format. One output block may require several source blocks,
    /// or contain only a converter tail; the tail and EOF are never observed.
    /// Observers that perform optional measurements should retain their own
    /// errors without interrupting playback.
    pub fn read_block_observed<E>(
        &mut self,
        mut observe: impl FnMut(PcmStreamFormat, &[f32]),
        process: impl FnOnce(&mut [f32]) -> Result<(), E>,
    ) -> Result<Option<PcmBlock<'_>>, StreamError<E>> {
        if self.ended {
            return Ok(None);
        }
        self.started = true;
        if let Some(converter) = &mut self.resampler {
            loop {
                let frames = self
                    .decoder
                    .read_frames(&mut self.samples)
                    .map_err(StreamError::Decode)?;
                let source_count = frames * usize::from(self.source_format.channels());
                if frames > 0 {
                    observe(self.source_format, &self.samples[..source_count]);
                }
                let count = frames * usize::from(self.decode_format.channels());
                let samples = if let Some(downmix) = &self.downmix {
                    let stereo = &mut self.stereo[..count];
                    downmix
                        .process(&self.samples[..source_count], stereo)
                        .map_err(StreamError::Downmix)?;
                    stereo
                } else {
                    &mut self.samples[..count]
                };
                let output = converter
                    .push(samples, frames == 0)
                    .map_err(StreamError::Resample)?;
                if frames == 0 {
                    self.ended = true;
                }
                if !output.is_empty() {
                    self.converted.clear();
                    self.converted.extend_from_slice(output);
                    break;
                }
                if self.ended {
                    return Ok(None);
                }
            }
            return finish_block(
                &mut self.converted,
                &mut self.bytes,
                self.format.channels(),
                process,
            )
            .map(Some);
        }
        let frames = self
            .decoder
            .read_frames(&mut self.samples)
            .map_err(StreamError::Decode)?;
        if frames == 0 {
            self.ended = true;
            return Ok(None);
        }
        let source_count = frames * usize::from(self.source_format.channels());
        observe(self.source_format, &self.samples[..source_count]);
        let count = frames * usize::from(self.decode_format.channels());
        let samples = if let Some(downmix) = &self.downmix {
            let stereo = &mut self.stereo[..count];
            downmix
                .process(&self.samples[..source_count], stereo)
                .map_err(StreamError::Downmix)?;
            stereo
        } else {
            &mut self.samples[..count]
        };
        finish_block(samples, &mut self.bytes, self.format.channels(), process).map(Some)
    }
}

fn finish_block<'a, E>(
    samples: &'a mut [f32],
    bytes: &'a mut Vec<u8>,
    channels: u16,
    process: impl FnOnce(&mut [f32]) -> Result<(), E>,
) -> Result<PcmBlock<'a>, StreamError<E>> {
    process(samples).map_err(StreamError::Process)?;
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(StreamError::NonFiniteSample);
    }
    bytes.resize(samples.len() * 4, 0);
    for (sample, encoded) in samples.iter().zip(bytes.as_chunks_mut::<4>().0.iter_mut()) {
        encoded.copy_from_slice(&sample.to_le_bytes());
    }
    Ok(PcmBlock {
        samples,
        f32le: bytes,
        frames: samples.len() / usize::from(channels),
    })
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
