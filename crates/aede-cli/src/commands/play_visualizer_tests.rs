use super::{DISPLAY_BANDS, HEIGHT, MAX_COLUMNS, Peaks, Position, bar_rows, position_rows};
use aede_dsp::SPECTRUM_BANDS;
use std::time::Duration;

fn position(position_ms: u64, duration_ms: Option<u64>) -> Position<'static> {
    Position {
        token: 1,
        label: "Album — 01 song",
        position_ms,
        duration_ms,
        estimated: false,
        completed: false,
    }
}

#[test]
fn playback_position_shows_elapsed_total_and_a_proportional_progress_bar() {
    let start = position_rows(position(0, Some(240_000)), false, 79);
    assert_eq!(start[0], "Now: Album — 01 song");
    assert!(start[1].starts_with("Position: 00:00 / 04:00 ["));
    assert!(start[1].ends_with(" 0%"));
    assert!(!start[1].contains('━'));
    let middle = position_rows(position(120_000, Some(240_000)), false, 79);
    assert!(middle[1].contains("02:00 / 04:00"));
    assert!(middle[1].contains('━') && middle[1].contains('─'));
    assert!(middle[1].ends_with(" 50%"));
    assert_eq!(middle[1].chars().count(), 79);
    let end = position_rows(position(240_000, Some(240_000)), false, 79);
    assert!(end[1].ends_with(" 100%"));
    assert!(!end[1].contains('─'));
}

#[test]
fn pause_and_fallback_timing_are_visible_without_hiding_the_source_offset() {
    let mut held = position(3_671_000, Some(7_200_000));
    held.estimated = true;
    let rows = position_rows(held, true, 79);
    assert_eq!(rows[0], "Paused: Album — 01 song");
    assert!(rows[1].contains("Position: ~1:01:11 / 2:00:00"));
    let short_header = position_rows(position(5_000, Some(1_000)), false, 79);
    assert!(short_header[1].contains("00:05 / 00:01"));
    assert!(short_header[1].ends_with(" 100%"));
}

#[test]
fn unknown_duration_does_not_invent_a_percentage_or_progress_extent() {
    for duration in [None, Some(0)] {
        let rows = position_rows(position(72_000, duration), false, 79);
        assert_eq!(rows[1], "Position: 01:12 / --:--");
        assert!(!rows[1].contains(['%', '[', '━']));
    }
}

#[test]
fn position_text_is_bounded_and_cannot_inject_terminal_controls() {
    let mut current = position(u64::MAX, Some(u64::MAX));
    current.label = "bad\x1b[2J\nlabel";
    for columns in [1, 2, 20, 79, usize::MAX] {
        let rows = position_rows(current, false, columns);
        assert!(
            rows.iter()
                .all(|row| row.chars().count() <= columns.min(MAX_COLUMNS))
        );
        assert!(rows.iter().all(|row| !row.contains(['\x1b', '\n', '\r'])));
    }
}

#[test]
fn wide_segments_fill_from_the_bottom_at_their_own_heights() {
    let mut levels = [0.0; SPECTRUM_BANDS];
    levels[0] = 1.0;
    levels[2] = 0.55;
    let rows = bar_rows(&levels, &[0.0; SPECTRUM_BANDS], 47, false);
    assert_eq!(rows.len(), HEIGHT);
    assert!(rows[0].starts_with("▆▆▆    "));
    assert!(rows[HEIGHT / 2 - 2].starts_with("▆▆▆    "));
    assert!(rows[HEIGHT / 2 - 1].starts_with("▆▆▆ ▆▆▆"));
    assert!(rows[HEIGHT - 1].starts_with("▆▆▆ ▆▆▆"));
    assert!(rows.iter().all(|row| row.chars().count() == 47));
    assert!(rows.iter().all(|row| !row.contains(['┃', '╻'])));
}

#[test]
fn bars_fill_the_available_columns_and_resize_with_the_terminal() {
    let levels = [1.0; SPECTRUM_BANDS];
    for columns in [79, 119] {
        let rows = bar_rows(&levels, &[0.0; SPECTRUM_BANDS], columns, false);
        assert!(rows.iter().all(|row| row.chars().count() == columns));
        let widths: Vec<_> = rows[0].split(' ').map(|bar| bar.chars().count()).collect();
        assert_eq!(widths.len(), DISPLAY_BANDS);
        assert!(widths.iter().all(|width| *width >= 2));
    }
}

#[test]
fn narrow_terminals_keep_all_frequency_bands_without_wrapping() {
    for columns in [1, 2, 3, 11, 12, 20, 23] {
        for band in 0..SPECTRUM_BANDS {
            let mut levels = [0.0; SPECTRUM_BANDS];
            levels[band] = 1.0;
            let rows = bar_rows(&levels, &[0.0; SPECTRUM_BANDS], columns, false);
            assert!(rows.iter().all(|row| row.chars().count() == columns));
            assert!(
                rows[0].contains('▆'),
                "band {band} is lost at width {columns}"
            );
            if band == SPECTRUM_BANDS - 1 {
                assert!(rows[0].ends_with('▆'));
            }
        }
    }
}

#[test]
fn unbounded_terminal_width_cannot_request_an_unbounded_frame() {
    let rows = bar_rows(
        &[1.0; SPECTRUM_BANDS],
        &[0.0; SPECTRUM_BANDS],
        usize::MAX,
        false,
    );
    assert!(rows.iter().all(|row| row.chars().count() == MAX_COLUMNS));
}

#[test]
fn display_colors_follow_height_and_monochrome_keeps_the_same_segments() {
    let levels = [1.0; SPECTRUM_BANDS];
    let peaks = [0.0; SPECTRUM_BANDS];
    let colored = bar_rows(&levels, &peaks, 79, true);
    let plain = bar_rows(&levels, &peaks, 79, false);
    assert!(colored[0].contains("\x1b[31m"));
    assert!(colored[1].contains("\x1b[33m"));
    assert!(colored[2].contains("\x1b[33m"));
    assert!(colored[3].contains("\x1b[32m"));
    assert!(plain.iter().all(|row| !row.contains('\x1b')));
    for (colored, plain) in colored.iter().zip(plain) {
        let visible = ["\x1b[31m", "\x1b[33m", "\x1b[32m", "\x1b[0m"]
            .iter()
            .fold(colored.clone(), |text, code| text.replace(code, ""));
        assert_eq!(visible, plain);
    }
}

#[test]
fn held_peaks_remain_detached_and_readable_without_color() {
    let mut levels = [0.0; SPECTRUM_BANDS];
    let mut peaks = [0.0; SPECTRUM_BANDS];
    levels[0] = 0.2;
    peaks[1] = 0.85;
    let rows = bar_rows(&levels, &peaks, 47, false);
    assert!(rows[1].starts_with("▔▔▔"));
    assert!(rows[2].starts_with("   "));
    assert!(rows[HEIGHT - 1].starts_with("▆▆▆"));
}

#[test]
fn silence_has_neither_segments_nor_a_false_peak_marker() {
    let rows = bar_rows(&[0.0; SPECTRUM_BANDS], &[0.0; SPECTRUM_BANDS], 79, true);
    assert!(rows.iter().all(|row| row == &" ".repeat(79)));
}

#[test]
fn peaks_hold_then_fall_with_active_time() {
    let mut peaks = Peaks::default();
    peaks.update(&[1.0; SPECTRUM_BANDS], Duration::from_secs(1));
    peaks.update(&[0.0; SPECTRUM_BANDS], Duration::from_millis(1200));
    assert_eq!(peaks.levels, [1.0; SPECTRUM_BANDS]);
    peaks.update(&[0.0; SPECTRUM_BANDS], Duration::from_millis(1450));
    assert!(
        peaks
            .levels
            .iter()
            .all(|level| (level - 0.955).abs() < 0.000_01)
    );
    peaks.update(&[0.0; SPECTRUM_BANDS], Duration::from_secs(4));
    assert_eq!(peaks.levels, [0.0; SPECTRUM_BANDS]);
}

#[test]
fn peak_fall_is_independent_of_redraw_count_and_frozen_active_time() {
    let mut once = Peaks::default();
    let mut often = Peaks::default();
    once.update(&[1.0; SPECTRUM_BANDS], Duration::ZERO);
    often.update(&[1.0; SPECTRUM_BANDS], Duration::ZERO);
    once.update(&[0.0; SPECTRUM_BANDS], Duration::from_millis(1400));
    for step in 1..=14 {
        often.update(&[0.0; SPECTRUM_BANDS], Duration::from_millis(step * 100));
    }
    for (&once, &often) in once.levels.iter().zip(&often.levels) {
        assert!((once - 0.5275).abs() < 0.000_01);
        assert!((once - often).abs() < 0.000_01);
    }
    let held = often.levels;
    often.update(&[0.0; SPECTRUM_BANDS], Duration::from_millis(1400));
    assert_eq!(often.levels, held);
}

#[test]
fn a_new_peak_rises_immediately_and_restarts_its_hold() {
    let mut peaks = Peaks::default();
    peaks.update(&[0.4; SPECTRUM_BANDS], Duration::ZERO);
    peaks.update(&[0.8; SPECTRUM_BANDS], Duration::from_millis(300));
    assert_eq!(peaks.levels, [0.8; SPECTRUM_BANDS]);
    peaks.update(&[0.0; SPECTRUM_BANDS], Duration::from_millis(600));
    assert_eq!(peaks.levels, [0.8; SPECTRUM_BANDS]);
}
