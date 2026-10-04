//! Blocking decode/DSP production for a finite, occurrence-preserving queue.

use std::convert::Infallible;

use aede_core::playback::gain_plan::GainPlan;
use aede_core::playback::session::{PcmSession, SessionBlock};
use aede_core::playback::stream::PcmTrack;
use aede_dsp::{PcmFormat, gain_with_headroom_db};

use super::*;

fn emit(
    sender: &mpsc::Sender<ProducerEvent>,
    cancelled: &AtomicBool,
    event: ProducerEvent,
) -> Result<(), ProducerStop> {
    if cancelled.load(AtomicOrdering::Acquire) {
        return Err(ProducerStop::Cancelled);
    }
    sender
        .blocking_send(event)
        .map_err(|_| ProducerStop::Cancelled)
}

struct Output<'a> {
    sender: &'a mpsc::Sender<ProducerEvent>,
    cancelled: &'a AtomicBool,
    sources: &'a [TrackSource],
    started: Vec<bool>,
    sent: u64,
    interactive: bool,
    first_position_ms: u64,
}

impl Output<'_> {
    fn position(&self, index: usize) -> Option<u64> {
        self.interactive.then_some(if index == 0 {
            self.first_position_ms
        } else {
            0
        })
    }

    fn format(&self, format: PcmFormat, index: usize, queue: bool) -> Result<(), ProducerStop> {
        let source = self
            .sources
            .get(index)
            .ok_or(ProducerStop::Failed(StreamFailure::STREAM_FAILED))?;
        emit(
            self.sender,
            self.cancelled,
            ProducerEvent::Format(FormatFrame {
                kind: "format",
                track: source.reference.to_token(),
                encoding: "f32le",
                sample_rate: format.sample_rate(),
                channels: format.channels(),
                duration_ms: source.file.properties.duration_ms,
                max_unacknowledged_frames: u64::from(format.sample_rate())
                    * MAX_UNACKNOWLEDGED_MILLISECONDS
                    / 1_000,
                index: queue.then_some(index),
                start_frame: queue.then_some(self.sent),
                position_ms: self.position(index),
            }),
        )
    }

    fn block(&mut self, block: SessionBlock<'_>, format: PcmFormat) -> Result<(), ProducerStop> {
        let channels = usize::from(format.channels());
        let frame_bytes = channels
            .checked_mul(4)
            .filter(|width| *width > 0)
            .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
        let max_frames = (MAX_AUDIO_BYTES / frame_bytes).min(format.sample_rate() as usize);
        if max_frames == 0 {
            return Err(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED));
        }
        for span in block.spans {
            let position_ms = self.position(span.token);
            let source = self
                .sources
                .get(span.token)
                .ok_or(ProducerStop::Failed(StreamFailure::STREAM_FAILED))?;
            let started = self
                .started
                .get_mut(span.token)
                .ok_or(ProducerStop::Failed(StreamFailure::STREAM_FAILED))?;
            if !*started {
                emit(
                    self.sender,
                    self.cancelled,
                    ProducerEvent::Track(TrackFrame {
                        kind: "track",
                        index: span.token,
                        track: source.reference.to_token(),
                        start_frame: self.sent,
                        duration_ms: source.file.properties.duration_ms,
                        position_ms,
                    }),
                )?;
                *started = true;
            }
            if !span.samples.start.is_multiple_of(channels)
                || !span.samples.end.is_multiple_of(channels)
            {
                return Err(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED));
            }
            let start = span
                .samples
                .start
                .checked_mul(4)
                .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            let end = span
                .samples
                .end
                .checked_mul(4)
                .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            let bytes = block
                .f32le
                .get(start..end)
                .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            for chunk in bytes.chunks(max_frames * frame_bytes) {
                let frames = u64::try_from(chunk.len() / frame_bytes)
                    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
                self.sent = self
                    .sent
                    .checked_add(frames)
                    .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
                emit(
                    self.sender,
                    self.cancelled,
                    ProducerEvent::Audio {
                        bytes: chunk.to_vec(),
                        frames,
                    },
                )?;
            }
            if span.complete {
                // Sealed sources can remain buffered in the converter while a
                // following file decodes. Recheck before announcing completion.
                validate_source(source).map_err(ProducerStop::Failed)?;
                emit(
                    self.sender,
                    self.cancelled,
                    ProducerEvent::TrackEnd(TrackEndFrame {
                        kind: "track_end",
                        index: span.token,
                        track: source.reference.to_token(),
                        end_frame: self.sent,
                    }),
                )?;
            }
        }
        Ok(())
    }
}

fn prepare_track(
    source: &TrackSource,
    sources: &[TrackSource],
    index: usize,
    settings: &ProducerSettings,
    normalization: &mut ReadyNormalization,
) -> Result<(PcmTrack, f32), ProducerStop> {
    validate_source(source).map_err(ProducerStop::Failed)?;
    let track = PcmTrack::open_stereo(&source.path)
        .map_err(|_| ProducerStop::Failed(StreamFailure::DECODE_FAILED))?;
    validate_source(source).map_err(ProducerStop::Failed)?;
    if settings.normalize == Mode::Album && index == 0 {
        // Album gain preparation can inspect every member of the exact queue
        // programme. Apply the same source policy before any future tag read.
        for source in sources {
            validate_source(source).map_err(ProducerStop::Failed)?;
        }
    }
    let gain = normalization
        .prepare(index)
        .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    validate_source(source).map_err(ProducerStop::Failed)?;
    if settings.normalize == Mode::Album && index == 0 {
        for source in sources {
            validate_source(source).map_err(ProducerStop::Failed)?;
        }
    }
    let gain = output_gain(gain, settings.tone)?;
    Ok((track, gain))
}

fn output_gain(gain: Option<GainPlan>, tone: ToneControls) -> Result<f32, ProducerStop> {
    let normalization = gain_with_headroom_db(
        gain.map_or(0.0, |gain| gain.gain_db),
        gain.and_then(|gain| gain.source_peak),
    )
    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    // Reserve tone headroom after capping normalization: applying the cap to
    // their sum could cancel the reserve when normalization requests a boost.
    Ok(normalization + tone.safe_preamp_db())
}

fn produce(
    sources: Vec<TrackSource>,
    settings: ProducerSettings,
    sender: mpsc::Sender<ProducerEvent>,
    cancelled: std::sync::Arc<AtomicBool>,
) -> Result<(), ProducerStop> {
    // Existing measurements and tags are reused without observing or saving a
    // new measurement. An arbitrary queue is not declared to be a whole album.
    let paths: Vec<_> = sources.iter().map(|source| source.path.clone()).collect();
    let mut normalization = ReadyNormalization::new(
        &paths,
        false,
        Some(&settings.normalization_catalog),
        &settings.data_dir,
        settings.normalize,
    )
    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
    let mut output = Output {
        sender: &sender,
        cancelled: &cancelled,
        sources: &sources,
        started: vec![false; sources.len()],
        sent: 0,
        interactive: settings.interactive,
        first_position_ms: 0,
    };
    let mut session: Option<PcmSession> = None;
    let mut fixed_rate = settings.output_rate;
    let mut output_format = None;
    for (index, source) in sources.iter().enumerate() {
        if cancelled.load(AtomicOrdering::Acquire) {
            return Err(ProducerStop::Cancelled);
        }
        let prepared = prepare_track(source, &sources, index, &settings, &mut normalization);
        let (mut track, gain) = match prepared {
            Ok(prepared) => prepared,
            Err(failure) => {
                // A completed preceding source keeps its valid converter tail
                // even when the next occurrence cannot be opened.
                if let Some(session) = &mut session {
                    let format = session.output_format();
                    let tail = session
                        .finish()
                        .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
                    output.block(tail, format)?;
                }
                return Err(failure);
            }
        };
        if index == 0 && settings.first_position_ms > 0 {
            let seek = track
                .seek_from_start(Duration::from_millis(settings.first_position_ms), || {
                    cancelled.load(AtomicOrdering::Acquire)
                })
                .map_err(|failure| match failure {
                    aede_core::playback::decoder::Error::SeekCancelled => ProducerStop::Cancelled,
                    aede_core::playback::decoder::Error::InvalidPosition => {
                        ProducerStop::Failed(StreamFailure::INVALID_SEEK)
                    }
                    _ => ProducerStop::Failed(StreamFailure::DECODE_FAILED),
                })?;
            if seek.reached_eof {
                return Err(ProducerStop::Failed(StreamFailure::INVALID_SEEK));
            }
            validate_source(source).map_err(ProducerStop::Failed)?;
            output.first_position_ms =
                frames_to_milliseconds(seek.frames, track.source_format().sample_rate());
        }
        let rate = *fixed_rate.get_or_insert(track.format().sample_rate().min(MAX_SAMPLE_RATE));
        if session
            .as_ref()
            .is_some_and(|session| !session.compatible(track.format(), rate, settings.tone))
        {
            if let Some(session) = &mut session {
                let format = session.output_format();
                let tail = session
                    .finish()
                    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
                output.block(tail, format)?;
            }
            session = None;
        }
        if session.is_none() {
            session = Some(
                PcmSession::new(track.format(), rate, settings.tone)
                    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?,
            );
        }
        let session = session
            .as_mut()
            .ok_or(ProducerStop::Failed(StreamFailure::STREAM_FAILED))?;
        let format = session.output_format();
        if output_format != Some(format) {
            output.format(format, index, settings.queue)?;
            output_format = Some(format);
        }
        session
            .begin_track(index, gain)
            .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
        loop {
            if cancelled.load(AtomicOrdering::Acquire) {
                return Err(ProducerStop::Cancelled);
            }
            let raw = track
                .read_block(|_| Ok::<(), Infallible>(()))
                .map_err(|_| ProducerStop::Failed(StreamFailure::DECODE_FAILED))?;
            let Some(raw) = raw else {
                validate_source(source).map_err(ProducerStop::Failed)?;
                let block = session
                    .end_track()
                    .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
                output.block(block, format)?;
                break;
            };
            let block = session
                .push_source(raw.samples)
                .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            output.block(block, format)?;
        }
    }
    if let Some(session) = &mut session {
        let format = session.output_format();
        let tail = session
            .finish()
            .map_err(|_| ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
        output.block(tail, format)?;
    }
    emit(
        &sender,
        &cancelled,
        ProducerEvent::Eof {
            frames: output.sent,
        },
    )
}

pub(super) fn spawn_producer(
    sources: Vec<TrackSource>,
    settings: ProducerSettings,
    cancelled: std::sync::Arc<AtomicBool>,
) -> (mpsc::Receiver<ProducerEvent>, tokio::task::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel(PRODUCER_EVENTS);
    let worker = tokio::task::spawn_blocking(move || {
        match produce(sources, settings, sender.clone(), cancelled) {
            Ok(()) | Err(ProducerStop::Cancelled) => {}
            Err(ProducerStop::Failed(failure)) => {
                let _ = sender.blocking_send(ProducerEvent::Error(failure));
            }
        }
    });
    (receiver, worker)
}

#[cfg(test)]
#[path = "playback_api_producer_tests.rs"]
mod tests;
