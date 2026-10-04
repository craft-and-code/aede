use super::{DISPLAY_BANDS, HEIGHT, MAX_COLUMNS, Peaks, bar_rows};
use aede_dsp::SPECTRUM_BANDS;
use std::time::Duration;

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
