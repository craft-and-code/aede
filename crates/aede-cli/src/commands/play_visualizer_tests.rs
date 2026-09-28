use super::{HEIGHT, bar_rows};
use aede_dsp::SPECTRUM_BANDS;

#[test]
fn slender_bars_fill_from_the_bottom_at_their_own_heights() {
    let mut levels = [0.0; SPECTRUM_BANDS];
    levels[0] = 1.0;
    levels[1] = 0.55;
    let rows = bar_rows(&levels, 47);
    assert_eq!(rows.len(), HEIGHT);
    assert!(rows[0].starts_with("┃  "));
    assert!(rows[HEIGHT / 2 - 1].starts_with("┃ ╻"));
    assert!(rows[HEIGHT / 2].starts_with("┃ ┃"));
    assert!(rows[HEIGHT - 1].starts_with("┃ ┃"));
    assert!(rows.iter().all(|row| row.chars().count() == 47));
    assert!(rows.iter().all(|row| !row.contains('█')));
}

#[test]
fn bars_fill_the_available_columns_and_resize_with_the_terminal() {
    let levels = [1.0; SPECTRUM_BANDS];
    for columns in [79, 119] {
        let rows = bar_rows(&levels, columns);
        assert!(rows.iter().all(|row| row.chars().count() == columns));
        let widths: Vec<_> = rows[0].split(' ').map(|bar| bar.chars().count()).collect();
        assert_eq!(widths.len(), SPECTRUM_BANDS);
        assert!(widths.iter().all(|width| *width >= 2));
    }
}

#[test]
fn narrow_terminals_keep_all_frequency_bands_without_wrapping() {
    let mut levels = [0.0; SPECTRUM_BANDS];
    levels[SPECTRUM_BANDS - 1] = 1.0;
    let rows = bar_rows(&levels, 20);
    assert!(rows.iter().all(|row| row.chars().count() == 20));
    assert!(rows[0].ends_with('┃'));
}
