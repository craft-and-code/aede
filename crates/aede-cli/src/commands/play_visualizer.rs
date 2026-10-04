//! Terminal-only rendering of frequency bands and the consumed playback position.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use aede_dsp::{PcmFormat, SPECTRUM_BANDS, Spectrum};

use super::timeline::Position;
use crate::ui;

const HEIGHT: usize = 10;
const POSITION_ROWS: usize = 2;
const DISPLAY_BANDS: usize = 12;
const MAX_COLUMNS: usize = 4096;
const REFRESH: Duration = Duration::from_millis(50);
const SIZE_REFRESH: Duration = Duration::from_millis(250);
const PEAK_HOLD: Duration = Duration::from_millis(350);
const PEAK_FALL_PER_SECOND: f32 = 0.45;

#[derive(Default)]
struct Peaks {
    levels: [f32; SPECTRUM_BANDS],
    held_until: [Duration; SPECTRUM_BANDS],
    updated_at: Duration,
}

impl Peaks {
    fn update(&mut self, levels: &[f32; SPECTRUM_BANDS], active_time: Duration) {
        for ((peak, held_until), level) in
            self.levels.iter_mut().zip(&mut self.held_until).zip(levels)
        {
            let fall = active_time
                .saturating_sub(self.updated_at.max(*held_until))
                .as_secs_f32()
                * PEAK_FALL_PER_SECOND;
            *peak = (*peak - fall).max(0.0);
            let level = level.clamp(0.0, 1.0);
            if level >= *peak {
                *peak = level;
                *held_until = active_time.saturating_add(PEAK_HOLD);
            }
        }
        self.updated_at = active_time;
    }
}

pub(super) struct TerminalVisualizer {
    spectrum: Spectrum,
    spectrum_enabled: bool,
    levels: [f32; SPECTRUM_BANDS],
    peaks: Peaks,
    last_draw: Option<Instant>,
    last_size_check: Instant,
    columns: usize,
    drawn: bool,
    last_paused: bool,
    disabled: bool,
}

impl TerminalVisualizer {
    pub(super) fn new(format: PcmFormat, spectrum_enabled: bool) -> Option<Self> {
        ui::is_interactive().then(|| Self {
            spectrum: Spectrum::new(format),
            spectrum_enabled,
            levels: [0.0; SPECTRUM_BANDS],
            peaks: Peaks::default(),
            last_draw: None,
            last_size_check: Instant::now(),
            columns: terminal_columns().saturating_sub(1).max(1),
            drawn: false,
            last_paused: false,
            disabled: false,
        })
    }

    pub(super) fn observe(&mut self, samples: &[f32], active_time: Duration) -> io::Result<()> {
        if self.disabled || !self.spectrum_enabled {
            return Ok(());
        }
        let Some(levels) = self.spectrum.push(samples).map_err(io::Error::other)? else {
            return Ok(());
        };
        // Keep transients even when the terminal redraw is throttled. Active
        // playback time also keeps the held peaks frozen during a pause.
        self.peaks.update(&levels, active_time);
        self.levels = levels;
        Ok(())
    }

    pub(super) fn render(
        &mut self,
        position: Option<Position<'_>>,
        paused: bool,
    ) -> io::Result<()> {
        if self.disabled {
            return Ok(());
        }
        let Some(position) = position else {
            return Ok(());
        };
        let now = Instant::now();
        if self.drawn
            && paused == self.last_paused
            && self
                .last_draw
                .is_some_and(|previous| now.duration_since(previous) < REFRESH)
        {
            return Ok(());
        }
        self.draw(position, paused)?;
        self.last_draw = Some(now);
        self.last_paused = paused;
        Ok(())
    }

    pub(super) fn disable(&mut self) {
        self.disabled = true;
        self.clear();
    }

    fn height(&self) -> usize {
        POSITION_ROWS + if self.spectrum_enabled { HEIGHT } else { 0 }
    }

    fn draw(&mut self, position: Position<'_>, paused: bool) -> io::Result<()> {
        if self.last_size_check.elapsed() >= SIZE_REFRESH {
            self.columns = terminal_columns().saturating_sub(1).max(1);
            self.last_size_check = Instant::now();
        }
        let mut output = io::stdout().lock();
        if self.drawn {
            write!(output, "\x1b[{}A", self.height())?;
        }
        if self.spectrum_enabled {
            for row in bar_rows(
                &self.levels,
                &self.peaks.levels,
                self.columns,
                ui::color_enabled(),
            ) {
                write!(output, "\r{row}\x1b[K\n")?;
            }
        }
        for row in position_rows(position, paused, self.columns) {
            write!(output, "\r{row}\x1b[K\n")?;
        }
        output.flush()?;
        self.drawn = true;
        Ok(())
    }

    pub(super) fn clear(&mut self) {
        if self.drawn {
            let mut output = io::stdout().lock();
            let _ = write!(output, "\x1b[{}A\r\x1b[J", self.height()).and_then(|_| output.flush());
            self.drawn = false;
        }
    }
}

impl Drop for TerminalVisualizer {
    fn drop(&mut self) {
        self.clear();
    }
}

fn position_rows(position: Position<'_>, paused: bool, columns: usize) -> [String; POSITION_ROWS] {
    let columns = columns.clamp(1, MAX_COLUMNS);
    let status = if paused { "Paused" } else { "Now" };
    let label = ui::truncate(
        &format!(
            "{status}: {}",
            ui::literal(position.label).replace(['\n', '\t'], " ")
        ),
        columns,
    );
    let elapsed = clock_text(position.position_ms);
    let total = position.duration_ms.filter(|&total| total > 0);
    let duration = total.map_or_else(|| "--:--".into(), clock_text);
    let estimate = if position.estimated { "~" } else { "" };
    let text = format!("Position: {estimate}{elapsed} / {duration}");
    let progress = if let Some(total) = total {
        // Only the bar is clamped to metadata's total; a short header must not
        // hide the actual source position observed on the output clock.
        let percent =
            (u128::from(position.position_ms.min(total)) * 100 / u128::from(total)) as usize;
        let suffix = format!(" {percent}%");
        let width = columns.saturating_sub(text.len() + suffix.len() + 3);
        if width > 0 {
            let filled = (u128::from(position.position_ms.min(total)) * width as u128
                / u128::from(total)) as usize;
            format!(
                "{text} [{}{}]{suffix}",
                "━".repeat(filled),
                "─".repeat(width - filled)
            )
        } else {
            format!("{text}{suffix}")
        }
    } else {
        text
    };
    [label, ui::truncate(&progress, columns)]
}

fn clock_text(milliseconds: u64) -> String {
    let seconds = milliseconds / 1000;
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}

fn bar_rows(
    levels: &[f32; SPECTRUM_BANDS],
    peaks: &[f32; SPECTRUM_BANDS],
    columns: usize,
    color: bool,
) -> [String; HEIGHT] {
    let columns = columns.clamp(1, MAX_COLUMNS);
    // Leave room for two-cell segments and a gap; merge neighbouring frequency
    // ranges on a narrow terminal rather than omitting the highest bands.
    let shown = DISPLAY_BANDS.min((columns + 1) / 3).max(1);
    let gap = usize::from(shown > 1);
    let bar_columns = columns - gap * (shown - 1);
    std::array::from_fn(|row| {
        let from_bottom = HEIGHT - row - 1;
        let mut line = String::with_capacity(columns * 3);
        for index in 0..shown {
            if index > 0 {
                for _ in 0..gap {
                    line.push(' ');
                }
            }
            let first_band = index * SPECTRUM_BANDS / shown;
            let next_band = (index + 1) * SPECTRUM_BANDS / shown;
            let level = levels[first_band..next_band]
                .iter()
                .copied()
                .fold(0.0_f32, f32::max);
            let peak = peaks[first_band..next_band]
                .iter()
                .copied()
                .fold(0.0_f32, f32::max);
            let full = (level.clamp(0.0, 1.0) * HEIGHT as f32).round() as usize;
            let marker = (peak.clamp(0.0, 1.0) * HEIGHT as f32) as usize;
            let (symbol, code) = if from_bottom < full {
                let code = if from_bottom >= 9 {
                    "31"
                } else if from_bottom >= 7 {
                    "33"
                } else {
                    "32"
                };
                ('▆', code)
            } else if peak > 0.0 && from_bottom == marker.min(HEIGHT - 1) {
                ('▔', "0")
            } else {
                (' ', "0")
            };
            let width = (index + 1) * bar_columns / shown - index * bar_columns / shown;
            let segment: String = std::iter::repeat_n(symbol, width).collect();
            if symbol == ' ' {
                line.push_str(&segment);
            } else {
                line.push_str(&ui::colorize_with_color(code, &segment, color));
            }
        }
        line
    })
}

pub(super) fn terminal_columns() -> usize {
    #[cfg(unix)]
    if let Ok(terminal) = std::fs::File::open("/dev/tty")
        && let Ok(output) = std::process::Command::new("stty")
            .arg("size")
            .stdin(terminal)
            .output()
        && output.status.success()
        && let Some(columns) = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<usize>().ok())
        && columns > 0
    {
        return columns.min(MAX_COLUMNS);
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|columns| *columns > 0)
        .unwrap_or(80)
        .min(MAX_COLUMNS)
}

#[cfg(test)]
#[path = "play_visualizer_tests.rs"]
mod tests;
