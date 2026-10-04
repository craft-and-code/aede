//! Track-occurrence attribution on the acknowledged socket output timeline.

use super::*;

struct Occurrence {
    index: usize,
    track: String,
    start: u64,
    end: Option<u64>,
    sample_rate: u32,
    started_at: u64,
    position_ms: u64,
}

#[derive(Default)]
pub(super) struct ListeningTimeline {
    occurrences: Vec<Occurrence>,
}

#[derive(Clone)]
pub(super) struct Listened {
    pub(super) index: usize,
    pub(super) source: TrackSource,
    pub(super) started_at: u64,
    pub(super) ms_played: u64,
    pub(super) completed: bool,
}

impl ListeningTimeline {
    pub(super) fn begin(
        &mut self,
        frame: &TrackFrame,
        sent: u64,
        sample_rate: u32,
        sources: &[TrackSource],
    ) -> Result<(), StreamFailure> {
        if frame.index != self.occurrences.len()
            || frame.index >= MAX_QUEUE_TRACKS
            || frame.start_frame != sent
            || sources
                .get(frame.index)
                .is_none_or(|source| source.reference.to_token() != frame.track)
            || self
                .occurrences
                .last()
                .is_some_and(|previous| previous.end != Some(sent))
        {
            return Err(StreamFailure::STREAM_FAILED);
        }
        self.occurrences.push(Occurrence {
            index: frame.index,
            track: frame.track.clone(),
            start: sent,
            end: None,
            sample_rate,
            started_at: clock::now_seconds(),
            position_ms: frame.position_ms.unwrap_or(0),
        });
        Ok(())
    }

    pub(super) fn end(&mut self, frame: &TrackEndFrame, sent: u64) -> Result<(), StreamFailure> {
        let occurrence = self
            .occurrences
            .last_mut()
            .ok_or(StreamFailure::STREAM_FAILED)?;
        if occurrence.index != frame.index
            || occurrence.track != frame.track
            || occurrence.end.is_some()
            || frame.end_frame != sent
            || sent < occurrence.start
        {
            return Err(StreamFailure::STREAM_FAILED);
        }
        occurrence.end = Some(sent);
        Ok(())
    }

    pub(super) fn output_rate(&self) -> Option<u32> {
        self.occurrences.first().map(|entry| entry.sample_rate)
    }

    pub(super) fn cursor(&self, consumed: u64) -> Option<(usize, u64, bool)> {
        let occurrence = self
            .occurrences
            .iter()
            .rev()
            .find(|entry| entry.start <= consumed)?;
        let at_end = occurrence.end.is_some_and(|end| end <= consumed);
        let frames = consumed
            .min(occurrence.end.unwrap_or(consumed))
            .saturating_sub(occurrence.start);
        Some((
            occurrence.index,
            occurrence
                .position_ms
                .saturating_add(frames_to_milliseconds(frames, occurrence.sample_rate)),
            at_end,
        ))
    }

    pub(super) fn listens(&self, consumed: u64, sources: &[TrackSource]) -> Vec<Listened> {
        self.acknowledged(consumed, sources)
            .into_iter()
            .map(|(listen, _, _)| listen)
            .filter(|listen| listen.ms_played > 0)
            .collect()
    }

    pub(super) fn acknowledged(
        &self,
        consumed: u64,
        sources: &[TrackSource],
    ) -> Vec<(Listened, u64, u32)> {
        self.occurrences
            .iter()
            .filter_map(|occurrence| {
                let consumed = consumed.min(occurrence.end.unwrap_or(consumed));
                let frames = consumed.saturating_sub(occurrence.start);
                let ms_played = frames_to_milliseconds(frames, occurrence.sample_rate);
                let source = sources.get(occurrence.index)?;
                (frames > 0).then(|| {
                    (
                        Listened {
                            index: occurrence.index,
                            source: source.clone(),
                            started_at: occurrence.started_at,
                            ms_played,
                            completed: occurrence.end.is_some_and(|end| consumed == end),
                        },
                        frames,
                        occurrence.sample_rate,
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "playback_api_history_tests.rs"]
mod tests;
