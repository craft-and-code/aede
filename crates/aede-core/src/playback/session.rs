//! Continuous processed PCM for successive compatible tracks.
//!
//! A session retains its rate converter and tone-filter state at natural track
//! boundaries. Decoding, source loudness observation, downmix and output timing
//! remain the driver's responsibility.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::ops::Range;

use aede_dsp::{Dsp, DspError, PcmFormat, ProcessStats, RateConverter, RateError, ToneControls};

/// A failure preparing or producing continuous processed PCM.
#[derive(Debug)]
pub enum SessionError {
    /// The input format, gain, tone or PCM was rejected by the DSP.
    Dsp(DspError),
    /// Rate conversion could not accept or produce the requested PCM.
    Resample(RateError),
    /// The current track must be sealed before starting another or finishing.
    TrackActive,
    /// Source PCM and track endings require an active track.
    NoTrack,
    /// A finished session cannot accept another track or more source PCM.
    Finished,
    /// The stream exceeded the representable cumulative frame count.
    FrameCountOverflow,
    /// Converted frames could not be attributed to the decoded source timeline.
    OutputTimeline,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dsp(error) => write!(f, "{error}"),
            Self::Resample(error) => write!(f, "{error}"),
            Self::TrackActive => f.write_str("seal the active playback track first"),
            Self::NoTrack => f.write_str("no active playback track"),
            Self::Finished => f.write_str("continuous playback session has finished"),
            Self::FrameCountOverflow => f.write_str("continuous playback frame count overflowed"),
            Self::OutputTimeline => {
                f.write_str("converted PCM exceeded its playback track timeline")
            }
        }
    }
}

impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Dsp(error) => Some(error),
            Self::Resample(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DspError> for SessionError {
    fn from(error: DspError) -> Self {
        Self::Dsp(error)
    }
}

impl From<RateError> for SessionError {
    fn from(error: RateError) -> Self {
        Self::Resample(error)
    }
}

/// A track's contiguous region of one processed or exact output block.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackSpan {
    /// Driver-supplied identity for the source track or queue entry.
    pub token: usize,
    /// Interleaved sample indices in [`SessionBlock::samples`] or
    /// [`super::exact_session::ExactSessionBlock::samples`].
    pub samples: Range<usize>,
    /// Observed normalized peak and guard interventions. Exact spans observe
    /// the source peak without modifying samples and have zero interventions.
    pub stats: ProcessStats,
    /// This span ends the track's produced output, including delayed SRC frames
    /// in a processed session. It does not acknowledge device consumption.
    /// An empty range may deliver completion without additional PCM.
    pub complete: bool,
}

/// Processed samples and their exact track attribution.
pub struct SessionBlock<'a> {
    /// Finite, guarded, interleaved output PCM.
    pub samples: &'a [f32],
    /// The same samples encoded as little-endian `f32`.
    pub f32le: &'a [u8],
    /// Ordered spans covering every output sample exactly once. Empty spans
    /// report track completion without introducing samples.
    pub spans: &'a [TrackSpan],
}

struct PendingTrack {
    token: usize,
    gain_db: f32,
    end_frame: Option<u64>,
}

/// A continuous output processor for successive naturally ending tracks.
///
/// Input PCM has already been decoded and optionally downmixed. Compatible
/// tracks share the converter, fractional frame accounting and tone-filter
/// state. Gain is selected separately for each track after rate conversion.
/// Output frame boundaries are `ceil(cumulative source frames × rate ratio)`.
/// A discontinuity, format or tone change requires a new session.
///
/// Call `end_track` only after the source was completely decoded, then start
/// the next compatible track or call `finish` once at the end of the group.
/// Returned views remain valid until the next mutable session call. Incomplete
/// frames, non-finite input and lifecycle requests leave state unchanged; after a
/// processing, conversion or timeline failure, the driver must discard the
/// session rather than retrying partially consumed PCM.
pub struct PcmSession {
    input: PcmFormat,
    output: PcmFormat,
    tone: ToneControls,
    dsp: Dsp,
    converter: Option<RateConverter>,
    tracks: VecDeque<PendingTrack>,
    active: bool,
    source_frames: u64,
    output_frames: u64,
    finished: bool,
    samples: Vec<f32>,
    bytes: Vec<u8>,
    spans: Vec<TrackSpan>,
}

impl PcmSession {
    /// Prepare a fixed PCM format, optional rate conversion and persistent tone
    /// shelves outside the output callback.
    pub fn new(
        input: PcmFormat,
        output_rate: u32,
        tone: ToneControls,
    ) -> Result<Self, SessionError> {
        let output = PcmFormat::with_layout(output_rate, input.layout())?;
        let converter = if input.sample_rate() == output_rate {
            None
        } else {
            Some(RateConverter::new(input, output_rate)?)
        };
        let mut dsp = Dsp::new(output);
        dsp.set_tone(tone)?;
        Ok(Self {
            input,
            output,
            tone,
            dsp,
            converter,
            tracks: VecDeque::new(),
            active: false,
            source_frames: 0,
            output_frames: 0,
            finished: false,
            samples: Vec::new(),
            bytes: Vec::new(),
            spans: Vec::new(),
        })
    }

    /// Source PCM format after any downmix and before rate conversion.
    pub fn input_format(&self) -> PcmFormat {
        self.input
    }

    /// Guarded PCM format accepted by the output sink.
    pub fn output_format(&self) -> PcmFormat {
        self.output
    }

    /// Whether the next naturally advancing track can share this unfinished
    /// processing session. Channel positions are part of compatibility.
    pub fn compatible(&self, input: PcmFormat, output_rate: u32, tone: ToneControls) -> bool {
        !self.finished
            && input == self.input
            && output_rate == self.output.sample_rate()
            && tone == self.tone
    }

    /// Start a new source track using a fixed final gain in dB. The driver
    /// includes any normalization and tone headroom reserve in this gain.
    /// Pending output from earlier tracks retains their own gains.
    pub fn begin_track(&mut self, token: usize, gain_db: f32) -> Result<(), SessionError> {
        if self.finished {
            return Err(SessionError::Finished);
        }
        if self.active {
            return Err(SessionError::TrackActive);
        }
        let mut validation = Dsp::new(self.output);
        validation.set_gain_db(gain_db, 0)?;
        self.tracks.push_back(PendingTrack {
            token,
            gain_db,
            end_frame: None,
        });
        self.active = true;
        Ok(())
    }

    /// Supply complete finite source frames. A converter may retain source
    /// frames and return no output until more PCM arrives or the group ends.
    pub fn push_source(&mut self, samples: &[f32]) -> Result<SessionBlock<'_>, SessionError> {
        if self.finished {
            return Err(SessionError::Finished);
        }
        if !self.active {
            return Err(SessionError::NoTrack);
        }
        let channels = usize::from(self.input.channels());
        if !samples.len().is_multiple_of(channels) {
            return Err(DspError::IncompleteFrame.into());
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(DspError::NonFiniteSample.into());
        }
        let frames = u64::try_from(samples.len() / channels)
            .map_err(|_| SessionError::FrameCountOverflow)?;
        let source_frames = self
            .source_frames
            .checked_add(frames)
            .ok_or(SessionError::FrameCountOverflow)?;
        self.output_boundary(source_frames)?;
        self.clear_block();
        if let Some(converter) = &mut self.converter {
            self.samples
                .extend_from_slice(converter.push(samples, false)?);
        } else {
            self.samples.extend_from_slice(samples);
        }
        self.source_frames = source_frames;
        self.process_block()?;
        Ok(self.block())
    }

    /// Seal the active track's source boundary without flushing the converter
    /// or resetting filters. Completion is emitted only once every output
    /// frame assigned to this track has been produced.
    pub fn end_track(&mut self) -> Result<SessionBlock<'_>, SessionError> {
        if self.finished {
            return Err(SessionError::Finished);
        }
        if !self.active {
            return Err(SessionError::NoTrack);
        }
        let end_frame = self.output_boundary(self.source_frames)?;
        let track = self.tracks.back_mut().ok_or(SessionError::OutputTimeline)?;
        track.end_frame = Some(end_frame);
        self.active = false;
        self.clear_block();
        self.complete_ready_tracks(0)?;
        Ok(self.block())
    }

    /// Flush the converter once after every source track has been sealed.
    /// Repeated calls return an empty block without duplicate completions.
    pub fn finish(&mut self) -> Result<SessionBlock<'_>, SessionError> {
        if self.active {
            return Err(SessionError::TrackActive);
        }
        self.clear_block();
        if self.finished {
            return Ok(self.block());
        }
        if let Some(converter) = &mut self.converter {
            self.samples.extend_from_slice(converter.push(&[], true)?);
        }
        self.process_block()?;
        if !self.tracks.is_empty()
            || self.output_frames != self.output_boundary(self.source_frames)?
        {
            return Err(SessionError::OutputTimeline);
        }
        self.finished = true;
        Ok(self.block())
    }

    fn output_boundary(&self, frames: u64) -> Result<u64, SessionError> {
        let boundary = (u128::from(frames) * u128::from(self.output.sample_rate()))
            .div_ceil(u128::from(self.input.sample_rate()));
        u64::try_from(boundary).map_err(|_| SessionError::FrameCountOverflow)
    }

    fn clear_block(&mut self) {
        self.samples.clear();
        self.bytes.clear();
        self.spans.clear();
    }

    fn block(&self) -> SessionBlock<'_> {
        SessionBlock {
            samples: &self.samples,
            f32le: &self.bytes,
            spans: &self.spans,
        }
    }

    fn complete_ready_tracks(&mut self, sample_index: usize) -> Result<(), SessionError> {
        while let Some(track) = self.tracks.front() {
            let Some(end) = track.end_frame else {
                break;
            };
            if end < self.output_frames {
                return Err(SessionError::OutputTimeline);
            }
            if end != self.output_frames {
                break;
            }
            self.spans.push(TrackSpan {
                token: track.token,
                samples: sample_index..sample_index,
                stats: ProcessStats::default(),
                complete: true,
            });
            self.tracks.pop_front();
        }
        Ok(())
    }

    fn process_block(&mut self) -> Result<(), SessionError> {
        let channels = usize::from(self.output.channels());
        if !self.samples.len().is_multiple_of(channels) {
            return Err(SessionError::OutputTimeline);
        }
        let current_boundary = self.output_boundary(self.source_frames)?;
        let mut offset = 0;
        self.complete_ready_tracks(offset)?;
        while offset < self.samples.len() {
            let track = self.tracks.front().ok_or(SessionError::OutputTimeline)?;
            let boundary = track.end_frame.unwrap_or(current_boundary);
            let available = boundary
                .checked_sub(self.output_frames)
                .ok_or(SessionError::OutputTimeline)?;
            let frames = (self.samples.len() - offset) / channels;
            let accepted = frames.min(usize::try_from(available).unwrap_or(usize::MAX));
            if accepted == 0 {
                return Err(SessionError::OutputTimeline);
            }
            let end = offset + accepted * channels;
            let token = track.token;
            let gain_db = track.gain_db;
            let sealed = track.end_frame.is_some();
            self.dsp.set_gain_db(gain_db, 0)?;
            let stats = self
                .dsp
                .process_for_output(&mut self.samples[offset..end])?;
            self.output_frames = self
                .output_frames
                .checked_add(accepted as u64)
                .ok_or(SessionError::FrameCountOverflow)?;
            let complete = sealed && self.output_frames == boundary;
            self.spans.push(TrackSpan {
                token,
                samples: offset..end,
                stats,
                complete,
            });
            if complete {
                self.tracks.pop_front();
            }
            offset = end;
            self.complete_ready_tracks(offset)?;
        }
        self.bytes.reserve(self.samples.len() * 4);
        for sample in &self.samples {
            self.bytes.extend_from_slice(&sample.to_le_bytes());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
