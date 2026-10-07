//! Continuous, unmodified integer PCM and exact output representations.

use std::error::Error;
use std::fmt;

use aede_dsp::ProcessStats;

use super::exact_output::{
    ExactOutputError, ExactOutputFormat, ExactPcmAdapter, MAX_EXACT_BLOCK_FRAMES,
};
use super::format::IntegerPcmFormat;
use super::session::TrackSpan;

/// Failure preparing or producing the unmodified source stream.
#[derive(Debug)]
pub enum ExactSessionError {
    /// Source/output representation or a supplied block is ineligible.
    Output(ExactOutputError),
    /// Seal the active source before starting another or finishing.
    TrackActive,
    /// Source blocks and endings require an active source occurrence.
    NoTrack,
    /// A finished session cannot accept further source audio.
    Finished,
    /// The stream exceeded its representable cumulative frame count.
    FrameCountOverflow,
}

impl fmt::Display for ExactSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(error) => write!(f, "{error}"),
            Self::TrackActive => f.write_str("seal the active exact playback track first"),
            Self::NoTrack => f.write_str("no active exact playback track"),
            Self::Finished => f.write_str("exact playback session has finished"),
            Self::FrameCountOverflow => f.write_str("exact playback frame count overflowed"),
        }
    }
}

impl Error for ExactSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Output(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ExactOutputError> for ExactSessionError {
    fn from(error: ExactOutputError) -> Self {
        Self::Output(error)
    }
}

/// Source-depth integers, exact encoded bytes and occurrence attribution.
pub struct ExactSessionBlock<'a> {
    /// Unscaled, interleaved source integers; these are never processed.
    pub samples: &'a [i32],
    /// The same frames encoded in the session's exact output representation.
    pub encoded: &'a [u8],
    /// Shared occurrence spans covering the source samples exactly once.
    /// Empty completion spans describe source EOF, not device consumption.
    pub spans: &'a [TrackSpan],
}

/// A fixed-format source session with no modifying processing stages.
///
/// This is a software building block, separate from [`super::session::PcmSession`].
/// It owns no decoder, transport, sound device or listening records. The existing
/// driver remains responsible for selection, cancellation/seeking, queue writes
/// and history after accepted output. There are no gain, tone, rate conversion,
/// downmix, guard, limiter or dither controls to silently apply or ignore here.
///
/// Call `end_track` only after successful decoder EOF, including a present FLAC
/// MD5 verification. Each occurrence completes once, without delayed frames or
/// flushing silence. Invalid blocks/lifecycle requests leave state unchanged.
/// A transport interruption or output failure requires discarding this session
/// and queued audio. Neither its output metadata nor produced completion markers
/// establish device eligibility, audible completion or physical bit identity.
pub struct ExactPcmSession {
    adapter: ExactPcmAdapter,
    active: Option<usize>,
    source_frames: u64,
    finished: bool,
    samples: Vec<i32>,
    bytes: Vec<u8>,
    spans: Vec<TrackSpan>,
}

impl ExactPcmSession {
    /// Validate fixed source/output representation outside the audio callback.
    pub fn new(
        input: IntegerPcmFormat,
        output: ExactOutputFormat,
    ) -> Result<Self, ExactSessionError> {
        let adapter = ExactPcmAdapter::new(input, output)?;
        let capacity = MAX_EXACT_BLOCK_FRAMES * usize::from(input.channels());
        let mut samples = Vec::new();
        samples
            .try_reserve(capacity)
            .map_err(ExactOutputError::Allocation)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve(capacity * output.representation().bytes_per_sample())
            .map_err(ExactOutputError::Allocation)?;
        let mut spans = Vec::new();
        spans.try_reserve(1).map_err(ExactOutputError::Allocation)?;
        Ok(Self {
            adapter,
            active: None,
            source_frames: 0,
            finished: false,
            samples,
            bytes,
            spans,
        })
    }

    /// Original precision, rate and channel association.
    pub fn input_format(&self) -> IntegerPcmFormat {
        self.adapter.source_format()
    }

    /// Exact representation metadata, distinct from device-route evidence.
    pub fn output_format(&self) -> ExactOutputFormat {
        self.adapter.output_format()
    }

    /// Cumulative accepted source frames, with no converted or delayed frames.
    pub fn source_frames(&self) -> u64 {
        self.source_frames
    }

    /// Whether a natural next occurrence can share this fixed-format session.
    pub fn compatible(&self, input: IntegerPcmFormat, output: ExactOutputFormat) -> bool {
        !self.finished && input == self.input_format() && output == self.output_format()
    }

    /// Start a driver-identified occurrence without selecting any processing.
    pub fn begin_track(&mut self, token: usize) -> Result<(), ExactSessionError> {
        if self.finished {
            return Err(ExactSessionError::Finished);
        }
        if self.active.is_some() {
            return Err(ExactSessionError::TrackActive);
        }
        self.active = Some(token);
        self.clear_block();
        Ok(())
    }

    /// Preserve one bounded block of complete source frames and encode exactly.
    ///
    /// The adapter checks source-depth range, frame alignment and block bounds
    /// before replacing any output. Observational peaks use an independent
    /// normalized value; canonical source integers are never altered.
    pub fn push_source(
        &mut self,
        samples: &[i32],
    ) -> Result<ExactSessionBlock<'_>, ExactSessionError> {
        if self.finished {
            return Err(ExactSessionError::Finished);
        }
        let token = self.active.ok_or(ExactSessionError::NoTrack)?;
        let channels = usize::from(self.input_format().channels());
        let frames = u64::try_from(samples.len() / channels)
            .map_err(|_| ExactSessionError::FrameCountOverflow)?;
        let source_frames = self
            .source_frames
            .checked_add(frames)
            .ok_or(ExactSessionError::FrameCountOverflow)?;
        self.adapter.encode_block(samples, &mut self.bytes)?;
        self.samples.clear();
        self.samples.extend_from_slice(samples);
        self.spans.clear();
        if !samples.is_empty() {
            // Every admitted 16/24-bit integer is exactly representable at
            // this power-of-two scale. This view observes, never processes.
            let scale = if self.input_format().bits_per_sample() == 16 {
                1.0 / 32768.0
            } else {
                1.0 / 8388608.0
            };
            let sample_peak = samples
                .iter()
                .map(|&sample| (sample as f32 * scale).abs())
                .fold(0.0, f32::max);
            self.spans.push(TrackSpan {
                token,
                samples: 0..samples.len(),
                stats: ProcessStats {
                    sample_peak,
                    overfull_samples: 0,
                },
                complete: false,
            });
        }
        self.source_frames = source_frames;
        Ok(self.block())
    }

    /// Seal one successfully decoded source, with no tail or extra samples.
    pub fn end_track(&mut self) -> Result<ExactSessionBlock<'_>, ExactSessionError> {
        if self.finished {
            return Err(ExactSessionError::Finished);
        }
        let token = self.active.ok_or(ExactSessionError::NoTrack)?;
        self.clear_block();
        self.active = None;
        self.spans.push(TrackSpan {
            token,
            samples: 0..0,
            stats: ProcessStats::default(),
            complete: true,
        });
        Ok(self.block())
    }

    /// Finish the sealed group; repeated calls emit no duplicate completion.
    pub fn finish(&mut self) -> Result<ExactSessionBlock<'_>, ExactSessionError> {
        if self.active.is_some() {
            return Err(ExactSessionError::TrackActive);
        }
        self.clear_block();
        self.finished = true;
        Ok(self.block())
    }

    fn clear_block(&mut self) {
        self.samples.clear();
        self.bytes.clear();
        self.spans.clear();
    }

    fn block(&self) -> ExactSessionBlock<'_> {
        ExactSessionBlock {
            samples: &self.samples,
            encoded: &self.bytes,
            spans: &self.spans,
        }
    }
}

#[cfg(test)]
#[path = "exact_session_tests.rs"]
mod tests;
