//! Reusable decoded PCM blocks for local and server playback drivers.
//!
//! This layer owns decoding and `f32le` encoding. The caller supplies optional
//! sample processing, an output sink, and any transport or progress controls.
//! It never opens a device or sends data over a network.

use std::error::Error;
use std::fmt;
use std::path::Path;

use super::decoder::{self, FileDecoder};
use super::format::PcmStreamFormat;
use aede_dsp::{DspError, StereoDownmixer};

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
    downmix: Option<StereoDownmixer>,
    samples: Vec<f32>,
    stereo: Vec<f32>,
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
            downmix: None,
            samples: vec![0.0; sample_capacity],
            stereo: Vec::new(),
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

    /// Read, optionally process, and encode the next block.
    ///
    /// Returns `None` only at end of file. The processing function is called
    /// once for each nonempty block and may retain state between calls. The
    /// returned views are valid until the next call to `read_block`.
    pub fn read_block<E>(
        &mut self,
        process: impl FnOnce(&mut [f32]) -> Result<(), E>,
    ) -> Result<Option<PcmBlock<'_>>, StreamError<E>> {
        let frames = self
            .decoder
            .read_frames(&mut self.samples)
            .map_err(StreamError::Decode)?;
        if frames == 0 {
            return Ok(None);
        }
        let source_count = frames * usize::from(self.source_format.channels());
        let count = frames * usize::from(self.format.channels());
        let samples = if let Some(downmix) = &self.downmix {
            let stereo = &mut self.stereo[..count];
            downmix
                .process(&self.samples[..source_count], stereo)
                .map_err(StreamError::Downmix)?;
            stereo
        } else {
            &mut self.samples[..count]
        };
        process(samples).map_err(StreamError::Process)?;
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(StreamError::NonFiniteSample);
        }
        let bytes = &mut self.bytes[..count * 4];
        for (sample, encoded) in samples.iter().zip(bytes.as_chunks_mut::<4>().0.iter_mut()) {
            encoded.copy_from_slice(&sample.to_le_bytes());
        }
        Ok(Some(PcmBlock {
            samples,
            f32le: bytes,
            frames,
        }))
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
