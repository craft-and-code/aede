//! Progressive file decoding for local and remote playback drivers.
//!
//! The decoder yields complete, finite, interleaved `f32` frames in the
//! source sample rate and channel layout. It does not own an audio device.

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::thread::JoinHandle;

use aede_dsp::ChannelLayout;
use flaccompagnon_core::AnalysisError;
use flaccompagnon_core::decode::PcmStreamDecoder;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

mod vorbis;

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
    Native(PcmStreamDecoder),
    Vorbis(vorbis::VorbisStream),
    Ffmpeg(FfmpegStream),
}

impl FileDecoder {
    /// Open a local file and read its format without decoding the whole track.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let (inner, sample_rate, channels) = if let Some(native) = vorbis::VorbisStream::open(path)?
        {
            let sample_rate = native.sample_rate;
            let channels = native.channels;
            (Source::Vorbis(native), sample_rate, channels)
        } else {
            match PcmStreamDecoder::open(path) {
                Ok(native) => {
                    let channels =
                        u16::try_from(native.channels).map_err(|_| Error::InvalidFormat)?;
                    let sample_rate = native.sample_rate;
                    (Source::Native(native), sample_rate, channels)
                }
                Err(native_error) => {
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
            (_, Source::Native(_)) => {
                ChannelLayout::from_mask(probe_channel_mask(path, sample_rate, channels)?)
                    .map_err(|_| Error::InvalidFormat)?
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
        if self.finished {
            return Ok(0);
        }

        while self.pending_offset == self.pending.len() {
            let chunk = match &mut self.inner {
                Source::Native(native) => native.next_chunk().map_err(Error::Decode)?,
                Source::Vorbis(native) => native.next_chunk()?,
                Source::Ffmpeg(external) => external.next_chunk()?,
            };
            let Some(chunk) = chunk else {
                self.finished = true;
                return Ok(0);
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

        let count = output.len().min(self.pending.len() - self.pending_offset);
        output[..count]
            .copy_from_slice(&self.pending[self.pending_offset..self.pending_offset + count]);
        self.pending_offset += count;
        Ok(count / channels)
    }
}

fn probe_channel_mask(path: &Path, sample_rate: u32, channels: u16) -> Result<u32, Error> {
    let file =
        std::fs::File::open(path).map_err(|error| Error::ChannelLayout(error.to_string()))?;
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
        hint.with_extension(extension);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| Error::ChannelLayout(error.to_string()))?;
    let track = probed
        .format
        .default_track()
        .ok_or_else(|| Error::ChannelLayout("no default audio track".into()))?;
    let mask = track
        .codec_params
        .channels
        .ok_or_else(|| Error::ChannelLayout("no channel mask in the stream header".into()))?
        .bits();
    if mask.count_ones() != u32::from(channels)
        || track.codec_params.sample_rate != Some(sample_rate)
    {
        return Err(Error::ChannelLayout(
            "channel mask or sample rate disagrees with the decoder".into(),
        ));
    }
    Ok(mask)
}

/// FFmpeg handles unsupported native codecs and Vorbis streams whose final
/// granule needs more precise trimming than the current native path provides.
/// Stream data and errors are kept separate, so a failed decode never looks
/// like an ordinary end of file.
struct FfmpegStream {
    child: Child,
    stdout: ChildStdout,
    stderr: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    pending_bytes: Vec<u8>,
    frame_bytes: usize,
    done: bool,
}

impl FfmpegStream {
    fn open(path: &Path, sample_rate: u32, channels: u16) -> Result<Self, Error> {
        let ffmpeg = crate::ffmpeg::find()
            .ok_or_else(|| Error::External(crate::ffmpeg::missing("playback")))?;
        Self::open_with_program(path, sample_rate, channels, &ffmpeg)
    }

    fn open_with_program(
        path: &Path,
        sample_rate: u32,
        channels: u16,
        ffmpeg: &str,
    ) -> Result<Self, Error> {
        if sample_rate == 0 || channels == 0 {
            return Err(Error::InvalidFormat);
        }
        let mut child = Command::new(ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:a:0",
                "-vn",
                "-sn",
                "-dn",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
            ])
            .args([
                "-ar",
                &sample_rate.to_string(),
                "-ac",
                &channels.to_string(),
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| Error::External(error.to_string()))?;
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::External("ffmpeg did not open its PCM pipe".into()));
        };
        let Some(mut stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::External("ffmpeg did not open its error pipe".into()));
        };
        let stderr = match std::thread::Builder::new()
            .name("aede-ffmpeg-errors".into())
            .spawn(move || {
                let mut tail = Vec::new();
                let mut buffer = [0; 1024];
                loop {
                    let count = stderr.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    tail.extend_from_slice(&buffer[..count]);
                    if tail.len() > 4096 {
                        tail.drain(..tail.len() - 4096);
                    }
                }
                Ok(tail)
            }) {
            Ok(handle) => handle,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::External(error.to_string()));
            }
        };
        Ok(Self {
            child,
            stdout,
            stderr: Some(stderr),
            pending_bytes: Vec::new(),
            frame_bytes: usize::from(channels) * 4,
            done: false,
        })
    }

    fn next_chunk(&mut self) -> Result<Option<Vec<f32>>, Error> {
        let mut buffer = [0; 16_384];
        loop {
            let complete = self.pending_bytes.len() / self.frame_bytes * self.frame_bytes;
            if complete > 0 {
                let bytes = self.pending_bytes.drain(..complete).collect::<Vec<_>>();
                let chunk = bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| f32::from_le_bytes(*bytes))
                    .collect();
                return Ok(Some(chunk));
            }
            let count = self
                .stdout
                .read(&mut buffer)
                .map_err(|error| Error::External(error.to_string()))?;
            if count == 0 {
                let status = self
                    .child
                    .wait()
                    .map_err(|error| Error::External(error.to_string()))?;
                self.done = true;
                let tail = match self.stderr.take() {
                    Some(handle) => handle
                        .join()
                        .map_err(|_| Error::External("ffmpeg error reader stopped".into()))?
                        .map_err(|error| Error::External(error.to_string()))?,
                    None => Vec::new(),
                };
                if !status.success() {
                    let message = String::from_utf8_lossy(&tail);
                    return Err(Error::External(format!(
                        "ffmpeg exited with {status}: {}",
                        message.trim()
                    )));
                }
                if !self.pending_bytes.is_empty() {
                    return Err(Error::IncompleteFrame);
                }
                return Ok(None);
            }
            self.pending_bytes.extend_from_slice(&buffer[..count]);
        }
    }
}

impl Drop for FfmpegStream {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(handle) = self.stderr.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
#[path = "decoder_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "decoder_vorbis_tests.rs"]
mod vorbis_tests;
