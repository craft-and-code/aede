//! Authenticated original-bit production; device framing belongs to the client.
use super::*;
use aede_audio::dop::Reader;
use std::fs::File;

fn current(source: &TrackSource, cancelled: &AtomicBool) -> Result<(), ProducerStop> {
    if cancelled.load(AtomicOrdering::Acquire) {
        return Err(ProducerStop::Cancelled);
    }
    validate_source(source).map_err(ProducerStop::Failed)
}

fn open(source: &TrackSource, cancelled: &AtomicBool) -> Result<Reader, ProducerStop> {
    current(source, cancelled)?;
    let file = File::open(&source.path)
        .map_err(|_| ProducerStop::Failed(StreamFailure::SOURCE_UNAVAILABLE))?;
    let metadata = file
        .metadata()
        .map_err(|_| ProducerStop::Failed(StreamFailure::SOURCE_CHANGED))?;
    let identity = user::PlaybackSource {
        size: source.file.size,
        mtime: source.file.mtime,
        mtime_ns: Some(source.mtime_subseconds),
    };
    if !identity.matches_metadata(&metadata) {
        return Err(ProducerStop::Failed(StreamFailure::SOURCE_CHANGED));
    }
    let reader = Reader::from_file(file)
        .map_err(|_| ProducerStop::Failed(StreamFailure::DSD_UNSUPPORTED))?;
    current(source, cancelled)?;
    Ok(reader)
}

pub(super) fn produce(
    sources: &[TrackSource],
    settings: &ProducerSettings,
    sender: &mpsc::Sender<ProducerEvent>,
    cancelled: &AtomicBool,
) -> Result<(), ProducerStop> {
    // The existing ACK/lease timeline has one clock. Inspect headers only,
    // retaining at most one descriptor; never resample or partially admit a
    // mixed-clock queue before discovering its incompatible successor.
    let mut clock = None;
    for source in sources {
        let reader = open(source, cancelled)?;
        let rate = reader.info().sample_rate;
        if clock.is_some_and(|previous| previous != rate) {
            return Err(ProducerStop::Failed(StreamFailure::DSD_UNSUPPORTED));
        }
        clock = Some(rate);
    }
    let mut sent = 0u64;
    for (index, source) in sources.iter().enumerate() {
        let mut reader = open(source, cancelled)?;
        let info = reader.info();
        if clock != Some(info.sample_rate) {
            return Err(ProducerStop::Failed(StreamFailure::SOURCE_CHANGED));
        }
        let requested = if index == 0 {
            settings.first_position_samples.map_or_else(
                || u128::from(settings.first_position_ms) * u128::from(info.sample_rate) / 1000,
                u128::from,
            )
        } else {
            0
        };
        if requested > u128::from(info.samples)
            || (index == 0
                && settings.first_position_samples.is_some()
                && !requested.is_multiple_of(16)
                && requested != u128::from(info.samples))
        {
            return Err(ProducerStop::Failed(StreamFailure::INVALID_SEEK));
        }
        let first_frame = if requested == u128::from(info.samples) {
            reader.frames()
        } else {
            (requested / 16) as u64
        };
        reader
            .seek(first_frame)
            .map_err(|_| ProducerStop::Failed(StreamFailure::DECODE_FAILED))?;
        let position = first_frame.saturating_mul(16).min(info.samples);
        let position_ms = frames_to_milliseconds(position, info.sample_rate);
        let rate = reader.format().carrier_rate();
        current(source, cancelled)?;
        producer::emit(
            sender,
            cancelled,
            ProducerEvent::Format(FormatFrame {
                kind: "format",
                track: source.reference.to_token(),
                encoding: "dsd16le",
                original: Some(DsdFacts {
                    source_rate: info.sample_rate,
                    source_samples: info.samples,
                    position_samples: position,
                }),
                sample_rate: rate,
                channels: info.channels as u16,
                duration_ms: Some(frames_to_milliseconds(info.samples, info.sample_rate)),
                max_unacknowledged_frames: u64::from(rate) * MAX_UNACKNOWLEDGED_MILLISECONDS / 1000,
                index: settings.queue.then_some(index),
                start_frame: settings.queue.then_some(sent),
                position_ms: settings.interactive.then_some(position_ms),
            }),
        )?;
        producer::emit(
            sender,
            cancelled,
            ProducerEvent::Track(TrackFrame {
                kind: "track",
                index,
                track: source.reference.to_token(),
                start_frame: sent,
                duration_ms: Some(frames_to_milliseconds(info.samples, info.sample_rate)),
                position_ms: settings.interactive.then_some(position_ms),
            }),
        )?;
        let mut track_frames = 0u64;
        let mut original_samples = 0u64;
        loop {
            let block = reader
                .next_chunk(|| {
                    if cancelled.load(AtomicOrdering::Acquire) {
                        Err(aede_audio::dsd::DsdError("DSD transport cancelled".into()))
                    } else {
                        Ok(())
                    }
                })
                .map_err(|_| {
                    if cancelled.load(AtomicOrdering::Acquire) {
                        ProducerStop::Cancelled
                    } else {
                        ProducerStop::Failed(StreamFailure::DECODE_FAILED)
                    }
                })?;
            current(source, cancelled)?;
            let Some(block) = block else { break };
            if !block.payload.len().is_multiple_of(info.channels)
                || block.first_frame != first_frame + track_frames
            {
                return Err(ProducerStop::Failed(StreamFailure::DECODE_FAILED));
            }
            let frames = (block.payload.len() / info.channels) as u64;
            let bytes: Vec<u8> = block
                .payload
                .iter()
                .flat_map(|word| word.0.to_le_bytes())
                .collect();
            if bytes.is_empty() || bytes.len() > MAX_AUDIO_BYTES {
                return Err(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED));
            }
            sent = sent
                .checked_add(frames)
                .ok_or(ProducerStop::Failed(StreamFailure::PROCESSING_FAILED))?;
            track_frames += frames;
            original_samples += block.valid_samples;
            producer::emit(sender, cancelled, ProducerEvent::Audio { bytes, frames })?;
        }
        if track_frames != reader.frames() - first_frame
            || original_samples != info.samples - position
        {
            return Err(ProducerStop::Failed(StreamFailure::DECODE_FAILED));
        }
        producer::emit(
            sender,
            cancelled,
            ProducerEvent::TrackEnd(TrackEndFrame {
                kind: "track_end",
                index,
                track: source.reference.to_token(),
                end_frame: sent,
            }),
        )?;
    }
    producer::emit(sender, cancelled, ProducerEvent::Eof { frames: sent })
}
