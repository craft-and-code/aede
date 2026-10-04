//! Bounded occurrence positions shared by terminal progress and lyric cues.

use std::collections::VecDeque;
use std::io;

use aede_dsp::PcmFormat;

use crate::ui;

pub(super) const MAX_PENDING: usize = 64;

struct Visit {
    token: usize,
    label: String,
    offset_ms: u64,
    duration_ms: Option<u64>,
    start_byte: Option<u64>,
    end_byte: u64,
    complete: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Position<'a> {
    pub(super) token: usize,
    pub(super) position_ms: u64,
    pub(super) duration_ms: Option<u64>,
    pub(super) label: &'a str,
    pub(super) estimated: bool,
    pub(super) completed: bool,
}

pub(super) struct PlaybackTimeline {
    visits: VecDeque<Visit>,
    last_completed: Option<Visit>,
    submitted_bytes: u64,
    format: Option<PcmFormat>,
    active_started_ms: Option<u64>,
}

impl PlaybackTimeline {
    pub(super) fn new() -> Self {
        Self {
            visits: VecDeque::new(),
            last_completed: None,
            submitted_bytes: 0,
            format: None,
            active_started_ms: None,
        }
    }

    pub(super) fn has_capacity(&self) -> bool {
        self.visits.len() < MAX_PENDING
    }

    pub(super) fn reset_output(&mut self) {
        self.visits.clear();
        self.last_completed = None;
        self.submitted_bytes = 0;
        self.format = None;
        self.active_started_ms = None;
    }

    pub(super) fn begin(
        &mut self,
        token: usize,
        format: PcmFormat,
        offset_ms: u64,
        duration_ms: Option<u64>,
        label: &str,
    ) -> io::Result<()> {
        if !self.has_capacity() {
            return Err(io::Error::other("playback lookahead is full"));
        }
        if self.format.is_some_and(|held| held != format) {
            return Err(io::Error::other(
                "playback output format changed without a reset",
            ));
        }
        self.format = Some(format);
        self.visits.push_back(Visit {
            token,
            label: ui::truncate(&ui::literal(label).replace(['\n', '\t'], " "), 80),
            offset_ms,
            duration_ms,
            start_byte: None,
            end_byte: self.submitted_bytes,
            complete: false,
        });
        Ok(())
    }

    pub(super) fn submitted(
        &mut self,
        token: usize,
        count: usize,
        active_ms: u64,
    ) -> io::Result<()> {
        let visit = self
            .visits
            .iter_mut()
            .find(|visit| visit.token == token)
            .ok_or_else(|| io::Error::other("playback output refers to an unknown occurrence"))?;
        if count > 0 {
            let next = self
                .submitted_bytes
                .checked_add(count as u64)
                .ok_or_else(|| io::Error::other("playback output counter exhausted"))?;
            visit.start_byte.get_or_insert(self.submitted_bytes);
            self.active_started_ms.get_or_insert(active_ms);
            self.submitted_bytes = next;
            visit.end_byte = next;
        }
        Ok(())
    }

    pub(super) fn completed(&mut self, token: usize) -> io::Result<()> {
        let visit = self
            .visits
            .iter_mut()
            .find(|visit| visit.token == token)
            .ok_or_else(|| io::Error::other("playback end refers to an unknown occurrence"))?;
        visit.complete = true;
        Ok(())
    }

    pub(super) fn set_duration(&mut self, token: usize, duration_ms: u64) -> io::Result<()> {
        let visit = self
            .visits
            .iter_mut()
            .find(|visit| visit.token == token)
            .ok_or_else(|| io::Error::other("playback duration refers to an unknown occurrence"))?;
        visit.duration_ms = Some(duration_ms);
        Ok(())
    }

    pub(super) fn position(
        &mut self,
        consumed_frames: Option<u64>,
        active_ms: u64,
    ) -> Option<Position<'_>> {
        let format = self.format?;
        let frame_bytes = u64::from(format.channels()) * 4;
        let submitted_frames = self.submitted_bytes / frame_bytes;
        let played_frames = consumed_frames
            .unwrap_or_else(|| {
                active_ms
                    .saturating_sub(self.active_started_ms.unwrap_or(active_ms))
                    .saturating_mul(u64::from(format.sample_rate()))
                    / 1000
            })
            .min(submitted_frames);
        while self
            .visits
            .front()
            .is_some_and(|visit| visit.complete && played_frames >= visit.end_byte / frame_bytes)
        {
            if let Some(visit) = self.visits.pop_front() {
                self.last_completed = Some(visit);
            }
        }
        let (visit, completed) = match self.visits.front() {
            Some(visit) if visit.start_byte.is_some() => (visit, false),
            _ => (self.last_completed.as_ref()?, true),
        };
        let start_frame = visit.start_byte.unwrap_or(visit.end_byte) / frame_bytes;
        if played_frames < start_frame {
            return None;
        }
        let position_ms = visit.offset_ms.saturating_add(
            played_frames
                .min(visit.end_byte / frame_bytes)
                .saturating_sub(start_frame)
                .saturating_mul(1000)
                / u64::from(format.sample_rate()),
        );
        Some(Position {
            token: visit.token,
            position_ms,
            duration_ms: visit.duration_ms,
            label: &visit.label,
            estimated: consumed_frames.is_none(),
            completed,
        })
    }
}

#[cfg(test)]
#[path = "play_timeline_tests.rs"]
mod tests;
