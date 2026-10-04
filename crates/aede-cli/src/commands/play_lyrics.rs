//! Local lyric cues follow consumed output, independently of decoder lookahead.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use aede_core::lyrics::{self, Lyrics, Timeline};
use aede_dsp::PcmFormat;

use crate::ui;

const MAX_PENDING: usize = 64;
const MAX_CUE_LINES: usize = 4;

struct Visit {
    token: usize,
    label: String,
    words: Option<Lyrics>,
    timeline: Option<Timeline>,
    offset_ms: u64,
    start_byte: Option<u64>,
    end_byte: u64,
    complete: bool,
    announced: bool,
    last_cue: Option<u64>,
}

pub(super) struct PlaybackLyrics {
    visits: VecDeque<Visit>,
    submitted_bytes: u64,
    format: Option<PcmFormat>,
    active_started_ms: Option<u64>,
    columns: usize,
    dimensions_checked: Instant,
}

impl PlaybackLyrics {
    pub(super) fn new() -> Self {
        Self {
            visits: VecDeque::new(),
            submitted_bytes: 0,
            format: None,
            active_started_ms: None,
            columns: super::visualizer::terminal_columns()
                .saturating_sub(1)
                .clamp(1, 512),
            dimensions_checked: Instant::now(),
        }
    }

    pub(super) fn has_capacity(&self) -> bool {
        self.visits.len() < MAX_PENDING
    }

    pub(super) fn reset_output(&mut self) {
        self.visits.clear();
        self.submitted_bytes = 0;
        self.format = None;
        self.active_started_ms = None;
    }

    pub(super) fn begin(
        &mut self,
        token: usize,
        path: &Path,
        current: Option<lyrics::CurrentTrack<'_>>,
        label: &str,
        format: PcmFormat,
        offset_ms: u64,
    ) -> io::Result<()> {
        let words = current.map_or_else(|| lyrics::read_local(path), lyrics::read_current);
        let words = match words {
            Ok(words) => words,
            Err(error) => {
                eprintln!("Lyrics unavailable: {error}");
                None
            }
        };
        self.enqueue(token, words, label, format, offset_ms)
    }

    fn enqueue(
        &mut self,
        token: usize,
        words: Option<Lyrics>,
        label: &str,
        format: PcmFormat,
        offset_ms: u64,
    ) -> io::Result<()> {
        if !self.has_capacity() {
            return Err(io::Error::other("lyric playback lookahead is full"));
        }
        if self.format.is_some_and(|held| held != format) {
            return Err(io::Error::other(
                "lyric output format changed without a reset",
            ));
        }
        self.format = Some(format);
        let timeline = words.as_ref().map(Timeline::new);
        self.visits.push_back(Visit {
            token,
            label: ui::truncate(&ui::literal(label).replace(['\n', '\t'], " "), 80),
            words,
            timeline,
            offset_ms,
            start_byte: None,
            end_byte: self.submitted_bytes,
            complete: false,
            announced: false,
            last_cue: None,
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
            .ok_or_else(|| io::Error::other("lyric output refers to an unknown occurrence"))?;
        if count > 0 {
            visit.start_byte.get_or_insert(self.submitted_bytes);
            self.active_started_ms.get_or_insert(active_ms);
            self.submitted_bytes = self
                .submitted_bytes
                .checked_add(count as u64)
                .ok_or_else(|| io::Error::other("lyric output counter exhausted"))?;
            visit.end_byte = self.submitted_bytes;
        }
        Ok(())
    }

    pub(super) fn completed(&mut self, token: usize) -> io::Result<()> {
        let visit = self
            .visits
            .iter_mut()
            .find(|visit| visit.token == token)
            .ok_or_else(|| io::Error::other("lyric end refers to an unknown occurrence"))?;
        visit.complete = true;
        Ok(())
    }

    fn updates(
        &mut self,
        consumed_frames: Option<u64>,
        active_ms: u64,
        columns: usize,
    ) -> Vec<String> {
        let Some(format) = self.format else {
            return Vec::new();
        };
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
            self.visits.pop_front();
        }
        let Some(visit) = self.visits.front_mut() else {
            return Vec::new();
        };
        let Some(start) = visit.start_byte else {
            return Vec::new();
        };
        let start_frame = start / frame_bytes;
        if played_frames < start_frame {
            return Vec::new();
        }
        let position_ms = visit.offset_ms.saturating_add(
            played_frames
                .saturating_sub(start_frame)
                .saturating_mul(1000)
                / u64::from(format.sample_rate()),
        );
        let mut rows = Vec::new();
        if !visit.announced {
            rows.push(format!("Lyrics: {}", visit.label));
            if consumed_frames.is_none() {
                rows.push("  Timing estimated from active playback; ffplay supplies no consumption counter".into());
            }
            visit.announced = true;
            if visit.timeline.as_ref().is_none_or(Timeline::is_empty) {
                match &visit.words {
                    None => rows.push("  No local lyrics".into()),
                    Some(words) => {
                        rows.push("  Untimed lyrics (preview)".into());
                        for line in words.lines.iter().take(MAX_CUE_LINES) {
                            rows.push(cue_text(&line.text, columns));
                        }
                        if words.lines.len() > MAX_CUE_LINES {
                            rows.push("  …".into());
                        }
                    }
                }
            }
        }
        if let Some(cue) = visit
            .timeline
            .as_ref()
            .and_then(|timeline| timeline.active_at(position_ms))
            && visit.last_cue != Some(cue.at_ms)
        {
            visit.last_cue = Some(cue.at_ms);
            rows.push(format!(
                "  [{:02}:{:02}.{:03}]",
                cue.at_ms / 60_000,
                cue.at_ms / 1000 % 60,
                cue.at_ms % 1000
            ));
            if let Some(words) = &visit.words {
                for &index in cue.line_indices.iter().take(MAX_CUE_LINES) {
                    if let Some(line) = words.lines.get(index) {
                        rows.push(cue_text(&line.text, columns));
                    }
                }
                if cue.line_indices.len() > MAX_CUE_LINES {
                    rows.push("  …".into());
                }
            }
        }
        rows.into_iter()
            .map(|row| ui::truncate(&row, columns.max(1)))
            .collect()
    }

    pub(super) fn render(
        &mut self,
        consumed_frames: Option<u64>,
        active_ms: u64,
    ) -> io::Result<()> {
        if self.dimensions_checked.elapsed() >= Duration::from_secs(1) {
            self.columns = super::visualizer::terminal_columns()
                .saturating_sub(1)
                .clamp(1, 512);
            self.dimensions_checked = Instant::now();
        }
        let rows = self.updates(consumed_frames, active_ms, self.columns);
        if !rows.is_empty() {
            let mut output = io::stdout().lock();
            for row in rows {
                writeln!(output, "{row}")?;
            }
            output.flush()?;
        }
        Ok(())
    }
}

fn cue_text(text: &str, columns: usize) -> String {
    if text.trim().is_empty() {
        return "  (no active words)".into();
    }
    format!(
        "  ▸ {}",
        ui::truncate(
            &ui::literal(text).replace(['\n', '\t'], " "),
            columns.saturating_sub(4).max(1)
        )
    )
}

#[cfg(test)]
#[path = "play_lyrics_tests.rs"]
mod tests;
