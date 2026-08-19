//! Terminal presentation: colours, aligned tables, bars.
//!
//! Two constraints guide this module. First, display width: an accented
//! character takes several bytes but a single column, so aligning on `len()`
//! would produce crooked tables. Then restraint: colours disappear as soon as
//! the output is not a terminal, so that `aede stats > file.txt` stays
//! readable.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

static COLOR_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn init_color(force_off: bool) {
    let enabled =
        !force_off && std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    COLOR_ENABLED.store(enabled, Ordering::Relaxed);
}

/// `true` when the output goes to a terminal.
///
/// A progress line redrawn with a carriage return is meant for a human
/// watching; piped into a file it turns into thousands of useless lines.
pub fn is_interactive() -> bool {
    std::io::stdout().is_terminal()
}

fn colorize(code: &str, text: &str) -> String {
    if COLOR_ENABLED.load(Ordering::Relaxed) {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    colorize("1", text)
}
pub fn dim(text: &str) -> String {
    colorize("2", text)
}
pub fn cyan(text: &str) -> String {
    colorize("36", text)
}
pub fn green(text: &str) -> String {
    colorize("32", text)
}
pub fn yellow(text: &str) -> String {
    colorize("33", text)
}
pub fn red(text: &str) -> String {
    colorize("31", text)
}

/// Approximate display width of a string, in terminal columns.
///
/// We count Unicode characters, not bytes, and give two columns to ideographs
/// and emoji. That is enough to align artist names.
pub fn display_width(text: &str) -> usize {
    text.chars()
        .filter(|c| !is_combining(*c))
        .map(|c| if is_wide(c) { 2 } else { 1 })
        .sum()
}

fn is_combining(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x20D0..=0x20FF)
}

fn is_wide(c: char) -> bool {
    matches!(
        c as u32,
        0x1100..=0x115F
            | 0x2E80..=0xA4CF
            | 0xAC00..=0xD7A3
            | 0xF900..=0xFAFF
            | 0xFF00..=0xFF60
            | 0xFFE0..=0xFFE6
            | 0x1F300..=0x1FAFF
    )
}

/// Truncates to `max` columns, adding an ellipsis when needed.
pub fn truncate(text: &str, max: usize) -> String {
    if display_width(text) <= max || max == 0 {
        return text.to_string();
    }
    let mut out = String::new();
    let mut width = 0;
    for c in text.chars() {
        let w = if is_wide(c) { 2 } else { 1 };
        if width + w > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        width += w;
    }
    out.push('…');
    out
}

/// Truncates to `max` columns by dropping the **start** of the text.
///
/// For a file path, the end is what identifies the file. Cutting the tail off
/// `/private/var/folders/94/hlcz0ry94lb6knr29wxlyt_c0000gn/T/bad.flac` leaves a
/// row that names no file at all.
pub fn truncate_start(text: &str, max: usize) -> String {
    if display_width(text) <= max || max == 0 {
        return text.to_string();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut width = 0;
    for c in text.chars().rev() {
        let w = if is_wide(c) { 2 } else { 1 };
        if width + w > max.saturating_sub(1) {
            break;
        }
        tail.push(c);
        width += w;
    }
    let mut out = String::from("…");
    out.extend(tail.into_iter().rev());
    out
}

/// Pads on the right up to `width` columns.
pub fn pad(text: &str, width: usize) -> String {
    let current = display_width(text);
    if current >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - current))
    }
}

/// Pads on the left up to `width` columns.
pub fn pad_left(text: &str, width: usize) -> String {
    let current = display_width(text);
    if current >= width {
        text.to_string()
    } else {
        format!("{}{text}", " ".repeat(width - current))
    }
}

/// Column alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Right,
}

/// Aligned table, with an underlined header.
pub struct Table {
    headers: Vec<String>,
    aligns: Vec<Align>,
    /// Maximum width per column; 0 = unlimited.
    limits: Vec<usize>,
    /// Columns whose overflow is cut from the start rather than from the end.
    keep_end: Vec<bool>,
    rows: Vec<Vec<String>>,
    show_header: bool,
}

impl Table {
    pub fn new(headers: &[&str]) -> Self {
        Table {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            aligns: vec![Align::Left; headers.len()],
            limits: vec![0; headers.len()],
            keep_end: vec![false; headers.len()],
            rows: Vec::new(),
            show_header: true,
        }
    }

    /// Table of values without a header row: for the "label → value" cards,
    /// where an empty header would be noise.
    pub fn plain(columns: usize) -> Self {
        let mut table = Table::new(&vec![""; columns]);
        table.show_header = false;
        table
    }

    pub fn align(mut self, index: usize, align: Align) -> Self {
        if let Some(slot) = self.aligns.get_mut(index) {
            *slot = align;
        }
        self
    }

    pub fn limit(mut self, index: usize, max: usize) -> Self {
        if let Some(slot) = self.limits.get_mut(index) {
            *slot = max;
        }
        self
    }

    /// Bounds a column like [`Table::limit`], but keeps its **end** when it
    /// overflows. For paths, where the file name is the informative part.
    pub fn path_limit(mut self, index: usize, max: usize) -> Self {
        if let Some(slot) = self.limits.get_mut(index) {
            *slot = max;
        }
        if let Some(slot) = self.keep_end.get_mut(index) {
            *slot = true;
        }
        self
    }

    pub fn push(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn render(&self) -> String {
        if self.rows.is_empty() {
            return dim("  (no results)\n");
        }
        let columns = self.headers.len();
        let mut widths: Vec<usize> = self.headers.iter().map(|h| display_width(h)).collect();

        let cells: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(i, cell)| {
                        let limit = self.limits.get(i).copied().unwrap_or(0);
                        match (limit > 0, self.keep_end.get(i).copied().unwrap_or(false)) {
                            (true, true) => truncate_start(cell, limit),
                            (true, false) => truncate(cell, limit),
                            (false, _) => cell.clone(),
                        }
                    })
                    .collect()
            })
            .collect();

        for row in &cells {
            for (i, cell) in row.iter().enumerate().take(columns) {
                widths[i] = widths[i].max(display_width(cell));
            }
        }

        let mut out = String::new();
        if !self.show_header {
            for row in &cells {
                self.render_row(&mut out, row, &widths);
            }
            return out;
        }
        // Header
        out.push_str("  ");
        for (i, header) in self.headers.iter().enumerate() {
            let text = match self.aligns[i] {
                Align::Left => pad(header, widths[i]),
                Align::Right => pad_left(header, widths[i]),
            };
            out.push_str(&bold(&text));
            if i + 1 < columns {
                out.push_str("  ");
            }
        }
        out.push('\n');
        // Rule
        out.push_str("  ");
        let rule: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
        out.push_str(&dim(&rule.join("  ")));
        out.push('\n');
        // Rows
        for row in &cells {
            self.render_row(&mut out, row, &widths);
        }
        out
    }

    /// Renders a data row, column by column, without trailing spaces (they
    /// pollute copy-paste and test comparisons).
    fn render_row(&self, out: &mut String, row: &[String], widths: &[usize]) {
        out.push_str("  ");
        let last = widths.len().saturating_sub(1);
        for (i, width) in widths.iter().enumerate() {
            let cell = row.get(i).map(String::as_str).unwrap_or("");
            let text = match self.aligns[i] {
                Align::Left => pad(cell, *width),
                Align::Right => pad_left(cell, *width),
            };
            out.push_str(&text);
            if i < last {
                out.push_str("  ");
            }
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
}

/// Proportional horizontal bar, for breakdowns.
pub fn bar(value: usize, max: usize, width: usize) -> String {
    if max == 0 || width == 0 {
        return String::new();
    }
    let filled = ((value * width + max / 2) / max).min(width);
    let rest = width - filled;
    // No escape sequence for an empty portion: it would pollute comparisons
    // and redirected output.
    if rest == 0 {
        "█".repeat(filled)
    } else {
        format!("{}{}", "█".repeat(filled), dim(&"·".repeat(rest)))
    }
}

/// Pluralises a noun: `plural(1, "album")` → "1 album".
pub fn plural(count: usize, singular: &str) -> String {
    if count > 1 {
        format!("{count} {singular}s")
    } else {
        format!("{count} {singular}")
    }
}

/// Section title.
pub fn section(title: &str) -> String {
    format!("\n{}\n", bold(&cyan(title)))
}

/// Readable percentage.
pub fn percent(ratio: f64) -> String {
    format!("{:.0} %", ratio * 100.0)
}

/// A measured elapsed time, in the largest unit that stays readable.
///
/// Milliseconds below a second, seconds below a minute, minutes beyond: a scan
/// that reports `260604 ms` makes the reader do the division.
pub fn elapsed(ms: u128) -> String {
    if ms < 1_000 {
        return format!("{ms} ms");
    }
    if ms < 60_000 {
        return format!("{:.1} s", ms as f64 / 1000.0);
    }
    let seconds = (ms + 500) / 1000;
    let (minutes, rest) = (seconds / 60, seconds % 60);
    if minutes < 60 {
        format!("{minutes} min {rest} s")
    } else {
        format!("{} h {} min", minutes / 60, minutes % 60)
    }
}

/// Long duration, in days/hours/minutes — and in seconds below a minute, so a
/// small library does not report "0 min".
pub fn long_duration(ms: u64) -> String {
    // Rounded like `text::format_duration`, so the two never disagree by a
    // second on the same value.
    let total_sec = (ms + 500) / 1000;
    let total_min = total_sec / 60;
    let days = total_min / 1440;
    let hours = (total_min % 1440) / 60;
    let minutes = total_min % 60;
    if days > 0 {
        format!("{days} d {hours} h {minutes} min")
    } else if hours > 0 {
        format!("{hours} h {minutes} min")
    } else if total_min > 0 {
        format!("{minutes} min {} s", total_sec % 60)
    } else {
        format!("{total_sec} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_time_stays_readable() {
        assert_eq!(elapsed(842), "842 ms");
        assert_eq!(elapsed(1_500), "1.5 s");
        // The one that prompted this: 260604 ms made the reader divide.
        assert_eq!(elapsed(260_604), "4 min 21 s");
        assert_eq!(elapsed(7_500_000), "2 h 5 min");
    }

    #[test]
    fn a_path_column_keeps_its_file_name() {
        // On macOS a temporary path is 60 columns of "/private/var/folders/…"
        // before the name even starts; cutting the tail names nothing.
        let long = "/private/var/folders/94/hlcz0ry94lb6knr29wxlyt_c0000gn/T/bad.flac";
        let cut = truncate_start(long, 20);
        assert!(cut.ends_with("bad.flac"), "kept: {cut}");
        assert!(cut.starts_with('…'));
        assert_eq!(display_width(&cut), 20);
        // Short enough to fit: nothing is touched.
        assert_eq!(truncate_start("short.flac", 20), "short.flac");
    }

    #[test]
    fn width_with_accents() {
        assert_eq!(display_width("Bjork"), 5);
        assert_eq!(display_width("Björk"), 5, "an accent takes one column");
        assert_eq!("Björk".len(), 6, "…but indeed two bytes");
    }

    #[test]
    fn alignment_with_accents() {
        // Without proper width handling, these two strings would be shifted
        // relative to each other.
        assert_eq!(display_width(&pad("Björk", 10)), 10);
        assert_eq!(display_width(&pad("Bjork", 10)), 10);
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("Kind of Blue", 20), "Kind of Blue");
        assert_eq!(truncate("Kind of Blue", 8), "Kind of…");
        assert_eq!(display_width(&truncate("Kind of Blue", 8)), 8);
    }

    #[test]
    fn empty_table() {
        let t = Table::new(&["A", "B"]);
        assert!(t.is_empty());
        assert!(t.render().contains("no results"));
    }

    #[test]
    fn aligned_table() {
        let mut t = Table::new(&["Artist", "Tracks"]).align(1, Align::Right);
        t.push(vec!["Björk".into(), "12".into()]);
        t.push(vec!["Miles Davis".into(), "3".into()]);
        let rendered = t.render();
        let lines: Vec<&str> = rendered.lines().collect();
        // Every data row must have the same useful width.
        assert!(lines.len() >= 4);
        assert!(rendered.contains("Björk"));
        assert!(rendered.contains("Miles Davis"));
    }

    #[test]
    fn plural_agreement() {
        assert_eq!(plural(0, "album"), "0 album");
        assert_eq!(plural(1, "album"), "1 album");
        assert_eq!(plural(3, "album"), "3 albums");
    }

    #[test]
    fn proportional_bar() {
        assert_eq!(bar(10, 10, 4), "████");
        assert_eq!(bar(0, 10, 4).chars().filter(|&c| c == '█').count(), 0);
        assert_eq!(bar(5, 10, 4).chars().filter(|&c| c == '█').count(), 2);
    }

    #[test]
    fn long_durations() {
        assert_eq!(long_duration(3_600_000), "1 h 0 min");
        assert_eq!(long_duration(90_000_000), "1 d 1 h 0 min");
        assert_eq!(long_duration(120_000), "2 min 0 s");
        assert_eq!(
            long_duration(40_000),
            "40 s",
            "below a minute, seconds are kept"
        );
    }

    #[test]
    fn table_without_header() {
        let mut t = Table::plain(2).align(1, Align::Right);
        t.push(vec!["Tracks".into(), "20".into()]);
        let rendered = t.render();
        assert!(
            !rendered.contains('─'),
            "no rule should appear: {rendered:?}"
        );
        assert_eq!(rendered.lines().count(), 1);
    }
}
