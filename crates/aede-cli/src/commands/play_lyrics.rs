//! Local lyric cues follow consumed output, independently of decoder lookahead.

use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::ui;
use aede_core::lyrics::{self, Lyrics, Timeline};

use super::timeline::Position;

const MAX_CUE_LINES: usize = 4;

struct Visit {
    token: usize,
    words: Option<Lyrics>,
    timeline: Option<Timeline>,
    announced: bool,
    last_cue: Option<u64>,
}

pub(super) struct PlaybackLyrics {
    visits: VecDeque<Visit>,
    columns: usize,
    dimensions_checked: Instant,
}

impl PlaybackLyrics {
    pub(super) fn new() -> Self {
        Self {
            visits: VecDeque::new(),
            columns: super::visualizer::terminal_columns()
                .saturating_sub(1)
                .clamp(1, 512),
            dimensions_checked: Instant::now(),
        }
    }

    pub(super) fn reset_output(&mut self) {
        self.visits.clear();
    }

    pub(super) fn begin(
        &mut self,
        token: usize,
        path: &Path,
        current: Option<lyrics::CurrentTrack<'_>>,
    ) -> io::Result<()> {
        let words = current.map_or_else(|| lyrics::read_local(path), lyrics::read_current);
        let words = match words {
            Ok(words) => words,
            Err(error) => {
                eprintln!("Lyrics unavailable: {error}");
                None
            }
        };
        self.enqueue(token, words);
        Ok(())
    }

    fn enqueue(&mut self, token: usize, words: Option<Lyrics>) {
        let timeline = words.as_ref().map(Timeline::new);
        self.visits.push_back(Visit {
            token,
            words,
            timeline,
            announced: false,
            last_cue: None,
        });
    }

    fn updates(&mut self, position: Option<Position<'_>>, columns: usize) -> Vec<String> {
        let Some(position) = position else {
            return Vec::new();
        };
        // The shared clock identifies the heard occurrence, so decoder
        // lookahead cannot advance the words ahead of buffered audio.
        let Some(index) = self
            .visits
            .iter()
            .position(|visit| visit.token == position.token)
        else {
            return Vec::new();
        };
        self.visits.drain(..index + usize::from(position.completed));
        if position.completed {
            return Vec::new();
        }
        let Some(visit) = self.visits.front_mut() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        if !visit.announced {
            rows.push(format!("Lyrics: {}", position.label));
            if position.estimated {
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
            .and_then(|timeline| timeline.active_at(position.position_ms))
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
        position: Option<Position<'_>>,
        before_write: impl FnOnce(),
    ) -> io::Result<()> {
        if self.dimensions_checked.elapsed() >= Duration::from_secs(1) {
            self.columns = super::visualizer::terminal_columns()
                .saturating_sub(1)
                .clamp(1, 512);
            self.dimensions_checked = Instant::now();
        }
        let rows = self.updates(position, self.columns);
        if !rows.is_empty() {
            before_write();
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
