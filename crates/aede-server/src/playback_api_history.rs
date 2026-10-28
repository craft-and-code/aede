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
    original: Option<DsdFacts>,
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
        original: Option<DsdFacts>,
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
            original,
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

    fn played(occurrence: &Occurrence, frames: u64) -> (u64, u32) {
        match occurrence.original {
            Some(original) => (
                frames.saturating_mul(16).min(
                    original
                        .source_samples
                        .saturating_sub(original.position_samples),
                ),
                original.source_rate,
            ),
            None => (frames, occurrence.sample_rate),
        }
    }

    pub(super) fn output_rate(&self) -> Option<u32> {
        self.occurrences.first().map(|entry| entry.sample_rate)
    }

    pub(super) fn original_cursor(&self, consumed: u64, seek_index: Option<usize>) -> Option<u64> {
        let occurrence = self
            .occurrences
            .iter()
            .rev()
            .find(|entry| entry.start <= consumed)?;
        let original = occurrence.original?;
        if seek_index.is_some_and(|index| index != occurrence.index)
            || (seek_index.is_none() && occurrence.end.is_some_and(|end| end <= consumed))
        {
            return Some(0);
        }
        let frames = consumed.saturating_sub(occurrence.start);
        let (played, _) = Self::played(occurrence, frames);
        Some(original.position_samples.saturating_add(played))
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
        let (played, rate) = Self::played(occurrence, frames);
        let position = occurrence.original.map_or_else(
            || {
                occurrence
                    .position_ms
                    .saturating_add(frames_to_milliseconds(played, rate))
            },
            |original| {
                frames_to_milliseconds(original.position_samples.saturating_add(played), rate)
            },
        );
        Some((occurrence.index, position, at_end))
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
                let (played, rate) = Self::played(occurrence, frames);
                let ms_played = frames_to_milliseconds(played, rate);
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
                        played,
                        rate,
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "playback_api_history_tests.rs"]
mod tests;
