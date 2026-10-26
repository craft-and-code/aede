//! Progressive file decoding for local and remote playback drivers.
//!
//! The decoder yields complete, finite, interleaved `f32` frames in the
//! source sample rate and channel layout. It does not own an audio device.
//! [`IntegerFileDecoder`] separately preserves source-depth integer PCM for
//! native 16/24-bit mono/stereo FLAC and WAV, without changing that float API.

use std::fmt;
use std::path::Path;

use aede_audio::decode::PcmStreamDecoder;
use aede_dsp::ChannelLayout;
use flaccompagnon_core::AnalysisError;

mod ffmpeg;
pub(crate) use ffmpeg::FfmpegStream;
mod flac;
mod flac_source;
mod integer;
mod integer_wav;
mod vorbis;

pub use integer::IntegerFileDecoder;

/// Verification of a FLAC's original integer PCM against its stored MD5.
///
/// This describes this decoder run, not a persisted audit or audio quality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlacMd5Status {
    /// STREAMINFO has no signature; successful decoding cannot verify MD5.
    NoSignature,
    /// A signature exists, but the complete source has not reached EOF.
    Pending,
    /// All decoded source samples match the signature at successful EOF.
    Verified,
    /// Complete decoding disagrees with the signature; reads return an error.
    Mismatch,
}

/// A file or buffer that cannot satisfy the playback PCM contract.
#[derive(Debug)]
pub enum Error {
    /// The underlying file could not be opened or decoded.
    Decode(AnalysisError),
    /// The file header describes an unusable PCM format.
    InvalidFormat,
    /// The caller's buffer is empty or cannot hold complete frames.
    InvalidBuffer,
    /// A decoded packet does not contain complete frames.
    IncompleteFrame,
    /// A decoded packet contains a non-finite sample.
    NonFiniteSample,
    /// A multichannel stream's speaker positions could not be established.
    ChannelLayout(String),
    /// An external decoder could not be started or did not finish cleanly.
    External(String),
    /// The requested position exceeds the representable source-frame count.
    InvalidPosition,
    /// Seeking requires a newly opened stream, before any PCM is read.
    SeekAfterStart,
    /// The caller stopped a progressive seek before reaching its target.
    SeekCancelled,
    /// The completely decoded FLAC audio disagrees with its STREAMINFO MD5.
    FlacMd5Mismatch,
    /// The source lies outside the initial exact integer PCM profile.
    UnsupportedIntegerSource(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "audio decode failed: {error}"),
            Self::InvalidFormat => f.write_str("audio file has an invalid PCM format"),
            Self::InvalidBuffer => f.write_str("output buffer must hold complete PCM frames"),
            Self::IncompleteFrame => f.write_str("decoded packet has an incomplete PCM frame"),
            Self::NonFiniteSample => f.write_str("decoded packet contains a non-finite sample"),
            Self::ChannelLayout(error) => write!(f, "cannot identify channel positions: {error}"),
            Self::External(error) => write!(f, "external audio decoder failed: {error}"),
            Self::InvalidPosition => f.write_str("seek position exceeds the source-frame range"),
            Self::SeekAfterStart => f.write_str("seek requires a newly opened PCM stream"),
            Self::SeekCancelled => f.write_str("audio seek was cancelled"),
            Self::FlacMd5Mismatch => {
                f.write_str("decoded FLAC audio does not match its STREAMINFO MD5")
            }
            Self::UnsupportedIntegerSource(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AnalysisError> for Error {
    fn from(error: AnalysisError) -> Self {
        Self::Decode(error)
    }
}

/// Decodes one local audio file progressively into caller-owned PCM buffers.
///
/// Each successful read produces at most one decoded packet. Short reads are
/// normal; only `Ok(0)` means end of stream. The caller can feed the returned
/// portion of the buffer to `aede-dsp` after creating a matching PCM format.
pub struct FileDecoder {
    inner: Source,
    sample_rate: u32,
    channels: u16,
    layout: ChannelLayout,
    pending: Vec<f32>,
    pending_offset: usize,
    finished: bool,
}

enum Source {
    Flac(flac::FlacStream),
    Native(PcmStreamDecoder),
    Vorbis(vorbis::VorbisStream),
    Ffmpeg(FfmpegStream),
}

/// Source frames traversed by a progressive seek.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeekResult {
    /// Complete source frames discarded before the next readable frame.
    pub frames: u64,
    /// The source ended before the requested position could be reached.
    pub reached_eof: bool,
}

impl FileDecoder {
    /// Open a local file and read its format without decoding the whole track.
    pub fn open(path: &Path) -> Result<Self, Error> {
        Self::open_with_fallback(path, true)
    }

    /// Background analysis never launches an external decoder with uncontrolled producer work.
    pub fn open_native(path: &Path) -> Result<Self, Error> {
        Self::open_with_fallback(path, false)
    }

    fn open_with_fallback(path: &Path, fallback: bool) -> Result<Self, Error> {
        // Refuse special files before the first reader can block while opening them.
        crate::copy::validate_source(path)
            .map_err(|error| Error::Decode(AnalysisError::Decode(error)))?;
        let (inner, sample_rate, channels) = if let Some(native) = vorbis::VorbisStream::open(path)?
        {
            let sample_rate = native.sample_rate;
            let channels = native.channels;
            (Source::Vorbis(native), sample_rate, channels)
        } else if let Some(native) = flac::FlacStream::open(path)? {
            let sample_rate = native.sample_rate;
            let channels = native.channels;
            (Source::Flac(native), sample_rate, channels)
        } else {
            match PcmStreamDecoder::open_for_playback(path) {
                Ok(native) => {
                    let channels =
                        u16::try_from(native.channels).map_err(|_| Error::InvalidFormat)?;
                    let sample_rate = native.sample_rate;
                    (Source::Native(native), sample_rate, channels)
                }
                Err(native_error) => {
                    let native_error = AnalysisError::Decode(native_error.to_string());
                    if !fallback {
                        return Err(Error::Decode(native_error));
                    }
                    let tags = match crate::tags::read(path).ok() {
                        Some(tags)
                            if matches!(
                                tags.properties.codec.as_str(),
                                "opus" | "aac" | "alac"
                            ) =>
                        {
                            tags
                        }
                        _ => return Err(Error::Decode(native_error)),
                    };
                    let sample_rate = tags.properties.sample_rate.ok_or(Error::InvalidFormat)?;
                    let channels = tags.properties.channels.ok_or(Error::InvalidFormat)?;
                    let external = FfmpegStream::open(path, sample_rate, channels)?;
                    (Source::Ffmpeg(external), sample_rate, channels)
                }
            }
        };
        if sample_rate == 0 || channels == 0 {
            return Err(Error::InvalidFormat);
        }
        let layout = match (channels, &inner) {
            (1, _) => ChannelLayout::MONO,
            (2, _) => ChannelLayout::STEREO,
            (_, Source::Vorbis(native)) => native.layout,
            (_, Source::Flac(native)) => native.layout,
            (_, Source::Native(native)) => {
                // Reuse the mask from the same protected probe as the decoded format.
                let mask = native.channel_mask.ok_or_else(|| {
                    Error::ChannelLayout("no channel mask in the stream header".into())
                })?;
                if mask.count_ones() != u32::from(channels) {
                    return Err(Error::ChannelLayout(
                        "channel mask disagrees with the decoder".into(),
                    ));
                }
                ChannelLayout::from_mask(mask).map_err(|_| Error::InvalidFormat)?
            }
            (_, Source::Ffmpeg(_)) => ChannelLayout::Unknown(channels),
        };
        Ok(Self {
            inner,
            sample_rate,
            channels,
            layout,
            pending: Vec::new(),
            pending_offset: 0,
            finished: false,
        })
    }

    /// Sample rate of the decoded PCM, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved channels in each frame.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Speaker positions, when the decoder exposes them.
    pub fn channel_layout(&self) -> ChannelLayout {
        self.layout
    }

    /// FLAC source MD5 status, or `None` for another codec.
    ///
    /// Verification uses original integer samples before float conversion or
    /// DSP and becomes conclusive only at EOF. Stopping early leaves it pending.
    /// Progressive seeking also decodes the discarded prefix and therefore
    /// still permits whole-source verification when the remainder reaches EOF.
    pub fn flac_md5_status(&self) -> Option<FlacMd5Status> {
        match &self.inner {
            Source::Flac(native) => Some(native.md5_status()),
            _ => None,
        }
    }

    /// Fill a buffer with complete frames and return the number of frames.
    ///
    /// The buffer must be nonempty and its length divisible by `channels()`.
    /// A rejected buffer is unchanged and does not advance decoding. A valid
    /// buffer may receive fewer frames than it can hold, even before EOF.
    pub fn read_frames(&mut self, output: &mut [f32]) -> Result<usize, Error> {
        let channels = usize::from(self.channels);
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

    /// Discard up to `frames` complete source frames from the current position.
    ///
    /// This uses the ordinary decoder, preserving encoder-delay and end-trim
    /// handling exactly. Memory stays bounded by one decoded packet, but work
    /// grows with the traversed audio. Cancellation is checked before decoding
    /// and after at most 4096 discarded frames; an individual decoder call may
    /// still block on I/O. On cancellation or a decode error, discard this
    /// decoder: its partially advanced position is not a successful seek.
    pub fn skip_frames(
        &mut self,
        frames: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<SeekResult, Error> {
        let channels = usize::from(self.channels);
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
        let channels = usize::from(self.channels);
        while self.pending_offset == self.pending.len() {
            let chunk = match &mut self.inner {
                Source::Flac(native) => native.next_chunk()?,
                Source::Native(native) => native
                    .next_chunk()
                    .map_err(|e| Error::Decode(AnalysisError::Decode(e.to_string())))?,
                Source::Vorbis(native) => native.next_chunk()?,
                Source::Ffmpeg(external) => external.next_chunk()?,
            };
            let Some(chunk) = chunk else {
                self.finished = true;
                return Ok(false);
            };
            if !chunk.len().is_multiple_of(channels) {
                return Err(Error::IncompleteFrame);
            }
            if chunk.iter().any(|sample| !sample.is_finite()) {
                return Err(Error::NonFiniteSample);
            }
            self.pending = chunk;
            self.pending_offset = 0;
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "decoder_vorbis_tests.rs"]
mod vorbis_tests;
