//! Terminal-only rendering of the playback frequency bands.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use aede_dsp::{PcmFormat, SPECTRUM_BANDS, Spectrum};

use crate::ui;

const HEIGHT: usize = 10;
const REFRESH: Duration = Duration::from_millis(50);
const SIZE_REFRESH: Duration = Duration::from_millis(250);

pub(super) struct TerminalVisualizer {
    spectrum: Spectrum,
    last_draw: Option<Instant>,
    last_size_check: Instant,
    columns: usize,
    drawn: bool,
    disabled: bool,
}

impl TerminalVisualizer {
    pub(super) fn new(format: PcmFormat) -> Option<Self> {
        ui::is_interactive().then(|| Self {
            spectrum: Spectrum::new(format),
            last_draw: None,
            last_size_check: Instant::now(),
            columns: terminal_columns().saturating_sub(1).max(1),
            drawn: false,
            disabled: false,
        })
    }

    pub(super) fn observe(&mut self, samples: &[f32]) -> io::Result<()> {
        if self.disabled {
            return Ok(());
        }
        let Some(levels) = self.spectrum.push(samples).map_err(io::Error::other)? else {
            return Ok(());
        };
        let now = Instant::now();
        if self
            .last_draw
            .is_some_and(|previous| now.duration_since(previous) < REFRESH)
        {
            return Ok(());
        }
        self.draw(&levels)?;
        self.last_draw = Some(now);
        Ok(())
    }

    pub(super) fn disable(&mut self) {
        self.disabled = true;
        self.clear();
    }

    fn draw(&mut self, levels: &[f32; SPECTRUM_BANDS]) -> io::Result<()> {
        if self.last_size_check.elapsed() >= SIZE_REFRESH {
            self.columns = terminal_columns().saturating_sub(1).max(1);
            self.last_size_check = Instant::now();
        }
        let mut output = io::stdout().lock();
        if self.drawn {
            write!(output, "\x1b[{HEIGHT}A")?;
        }
        for row in bar_rows(levels, self.columns) {
            write!(output, "\r{row}\x1b[K\n")?;
        }
        output.flush()?;
        self.drawn = true;
        Ok(())
    }

    fn clear(&mut self) {
        if self.drawn {
            let mut output = io::stdout().lock();
            let _ = write!(output, "\x1b[{HEIGHT}A\r\x1b[J").and_then(|_| output.flush());
            self.drawn = false;
        }
    }
}

impl Drop for TerminalVisualizer {
    fn drop(&mut self) {
        self.clear();
    }
}

fn bar_rows(levels: &[f32; SPECTRUM_BANDS], columns: usize) -> [String; HEIGHT] {
    let columns = columns.max(1);
    let shown = SPECTRUM_BANDS.min(columns);
    let gap = usize::from(columns >= shown * 2 - 1);
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
            let height = level.clamp(0.0, 1.0) * HEIGHT as f32;
            let full = height.floor() as usize;
            let symbol = if from_bottom < full {
                '┃'
            } else if from_bottom == full && height.fract() >= 0.5 {
                '╻'
            } else {
                ' '
            };
            let width = (index + 1) * bar_columns / shown - index * bar_columns / shown;
            for _ in 0..width {
                line.push(symbol);
            }
        }
        line
    })
}

fn terminal_columns() -> usize {
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
        return columns;
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|columns| *columns > 0)
        .unwrap_or(80)
}

#[cfg(test)]
#[path = "play_visualizer_tests.rs"]
mod tests;
