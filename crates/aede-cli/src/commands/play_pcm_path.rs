//! Source/session variants used by the one local transport driver.

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use aede_core::playback::decoder::{self, IntegerFileDecoder};
use aede_core::playback::exact_output::{
    ExactOutputFormat, ExactPcmAdapter, ExactSampleRepresentation,
};
use aede_core::playback::exact_session::{ExactPcmSession, ExactSessionBlock};
use aede_core::playback::format::IntegerPcmFormat;
use aede_core::playback::session::{PcmSession, SessionBlock, TrackSpan};
use aede_core::playback::stream::PcmTrack;
use aede_dsp::{PcmFormat, ToneControls};

const BLOCK_FRAMES: usize = 4096;

pub(super) enum SourceTrack {
    Processed(Box<PcmTrack>),
    Exact {
        decoder: Box<IntegerFileDecoder>,
        format: PcmFormat,
        samples: Vec<i32>,
    },
}

pub(super) enum SourceBlock<'a> {
    Processed { samples: &'a [f32], frames: usize },
    Exact { samples: &'a [i32], frames: usize },
}

impl SourceBlock<'_> {
    pub(super) fn frames(&self) -> usize {
        match self {
            Self::Processed { frames, .. } | Self::Exact { frames, .. } => *frames,
        }
    }
}

impl SourceTrack {
    pub(super) fn open(path: &Path, exact: bool) -> Result<Self, Box<dyn Error>> {
        if exact {
            let decoder = IntegerFileDecoder::open(path)?;
            let capacity = BLOCK_FRAMES * usize::from(decoder.format().channels());
            Ok(Self::Exact {
                format: PcmFormat::with_layout(
                    decoder.format().sample_rate(),
                    decoder.format().layout(),
                )?,
                decoder: Box::new(decoder),
                samples: vec![0; capacity],
            })
        } else {
            Ok(Self::Processed(Box::new(PcmTrack::open_stereo(path)?)))
        }
    }

    pub(super) fn format(&self) -> PcmFormat {
        match self {
            Self::Processed(track) => track.format(),
            Self::Exact { format, .. } => *format,
        }
    }

    pub(super) fn source_format(&self) -> PcmFormat {
        match self {
            Self::Processed(track) => track.source_format(),
            Self::Exact { format, .. } => *format,
        }
    }

    pub(super) fn integer_format(&self) -> Option<IntegerPcmFormat> {
        match self {
            Self::Processed(_) => None,
            Self::Exact { decoder, .. } => Some(decoder.format()),
        }
    }

    pub(super) fn seek_from_start(
        &mut self,
        position: Duration,
        cancelled: impl FnMut() -> bool,
    ) -> Result<decoder::SeekResult, decoder::Error> {
        match self {
            Self::Processed(track) => track.seek_from_start(position, cancelled),
            Self::Exact { decoder, .. } => {
                let rate = u128::from(decoder.format().sample_rate());
                let frames = u128::from(position.as_secs()) * rate
                    + u128::from(position.subsec_nanos()) * rate / 1_000_000_000;
                decoder.skip_frames(
                    u64::try_from(frames).map_err(|_| decoder::Error::InvalidPosition)?,
                    cancelled,
                )
            }
        }
    }

    pub(super) fn read_block_observed(
        &mut self,
        observe: impl FnMut(PcmFormat, &[f32]),
    ) -> Result<Option<SourceBlock<'_>>, Box<dyn Error>> {
        match self {
            Self::Processed(track) => Ok(track
                .read_block_observed(observe, |_| Ok::<(), std::convert::Infallible>(()))?
                .map(|block| SourceBlock::Processed {
                    samples: block.samples,
                    frames: block.frames,
                })),
            Self::Exact {
                decoder, samples, ..
            } => {
                // Strict playback has no source loudness measurement/cache.
                let frames = decoder.read_frames(samples)?;
                Ok((frames > 0).then(|| SourceBlock::Exact {
                    samples: &samples[..frames * usize::from(decoder.format().channels())],
                    frames,
                }))
            }
        }
    }
}

pub(in crate::commands::play) struct DriverBlock<'a> {
    pub(super) samples: &'a [f32],
    pub(super) integers: Option<&'a [i32]>,
    pub(super) exact_frame_bytes: Option<usize>,
    pub(super) bytes: &'a [u8],
    pub(super) spans: &'a [TrackSpan],
}

impl<'a> From<SessionBlock<'a>> for DriverBlock<'a> {
    fn from(block: SessionBlock<'a>) -> Self {
        Self {
            samples: block.samples,
            integers: None,
            exact_frame_bytes: None,
            bytes: block.f32le,
            spans: block.spans,
        }
    }
}

pub(super) enum Processing {
    Processed(Box<PcmSession>),
    Exact {
        session: Box<ExactPcmSession>,
        format: PcmFormat,
        observed: Vec<f32>,
    },
}

impl Processing {
    pub(super) fn processed(
        input: PcmFormat,
        output_rate: u32,
        tone: ToneControls,
    ) -> Result<Self, Box<dyn Error>> {
        Ok(Self::Processed(Box::new(PcmSession::new(
            input,
            output_rate,
            tone,
        )?)))
    }

    pub(super) fn exact(
        input: IntegerPcmFormat,
        route: ExactOutputFormat,
    ) -> Result<Self, Box<dyn Error>> {
        // The existing driver counts canonical 32-bit handoff bytes. The
        // native queue receives source-depth integers and independently maps
        // them to the route representation; these bytes are not device bytes.
        ExactPcmAdapter::new(input, route)?;
        let handoff = ExactOutputFormat::new(
            route.sample_rate(),
            route.layout(),
            ExactSampleRepresentation::Signed32Le,
            route.effective_bits().map(|bits| bits.min(32)),
        )?;
        let session = ExactPcmSession::new(input, handoff)?;
        Ok(Self::Exact {
            format: PcmFormat::with_layout(input.sample_rate(), input.layout())?,
            session: Box::new(session),
            observed: Vec::with_capacity(BLOCK_FRAMES * usize::from(input.channels())),
        })
    }

    pub(super) fn compatible(&self, track: &SourceTrack) -> bool {
        match (self, track.integer_format()) {
            (Self::Processed(session), None) => session.input_format() == track.format(),
            (Self::Exact { session, .. }, Some(input)) => session.input_format() == input,
            _ => false,
        }
    }

    pub(super) fn output_format(&self) -> PcmFormat {
        match self {
            Self::Processed(session) => session.output_format(),
            Self::Exact { format, .. } => *format,
        }
    }

    pub(super) fn begin_track(&mut self, token: usize, gain_db: f32) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Processed(session) => Ok(session.begin_track(token, gain_db)?),
            Self::Exact { session, .. } => {
                if gain_db != 0.0 {
                    return Err("exact playback cannot apply gain".into());
                }
                Ok(session.begin_track(token)?)
            }
        }
    }

    pub(super) fn push_source(
        &mut self,
        source: SourceBlock<'_>,
    ) -> Result<DriverBlock<'_>, Box<dyn Error>> {
        match (self, source) {
            (Self::Processed(session), SourceBlock::Processed { samples, .. }) => {
                Ok(session.push_source(samples)?.into())
            }
            (
                Self::Exact {
                    session, observed, ..
                },
                SourceBlock::Exact { samples, .. },
            ) => {
                let format = session.input_format();
                Ok(observe_exact(
                    session.push_source(samples)?,
                    format,
                    observed,
                ))
            }
            _ => Err("source and playback policy disagree".into()),
        }
    }

    pub(super) fn end_track(&mut self) -> Result<DriverBlock<'_>, Box<dyn Error>> {
        match self {
            Self::Processed(session) => Ok(session.end_track()?.into()),
            Self::Exact {
                session, observed, ..
            } => {
                let format = session.input_format();
                Ok(observe_exact(session.end_track()?, format, observed))
            }
        }
    }

    pub(super) fn finish(&mut self) -> Result<DriverBlock<'_>, Box<dyn Error>> {
        match self {
            Self::Processed(session) => Ok(session.finish()?.into()),
            Self::Exact {
                session, observed, ..
            } => {
                let format = session.input_format();
                Ok(observe_exact(session.finish()?, format, observed))
            }
        }
    }
}

fn observe_exact<'a>(
    block: ExactSessionBlock<'a>,
    format: IntegerPcmFormat,
    observed: &'a mut Vec<f32>,
) -> DriverBlock<'a> {
    observed.clear();
    let scale = if format.bits_per_sample() == 16 {
        1.0 / 32768.0
    } else {
        1.0 / 8388608.0
    };
    observed.extend(block.samples.iter().map(|&sample| sample as f32 * scale));
    DriverBlock {
        samples: observed,
        integers: Some(block.samples),
        exact_frame_bytes: Some(usize::from(format.channels()) * 4),
        bytes: block.encoded,
        spans: block.spans,
    }
}

#[cfg(test)]
#[path = "play_pcm_path_tests.rs"]
mod tests;
