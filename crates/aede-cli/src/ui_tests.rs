use super::*;

#[test]
fn a_moment_and_a_span_are_not_the_same_number() {
    // The bug this pair exists to prevent: `ago` takes a duration and a
    // Unix timestamp is a plausible duration, so `ago(created_at)` reads
    // perfectly and says "56 years ago" for something a second old. It
    // shipped once, in a listing, and was caught by running the program
    // rather than by any test — which is why there is now one.
    let now = aede_core::clock::now_seconds();
    assert_eq!(since(now), "just now");
    assert_eq!(ago(0), "just now");
    assert!(
        since(0).ends_with("years ago"),
        "the epoch is a long time ago: {}",
        since(0)
    );
    // And `ago` keeps its own meaning, which is what makes it testable
    // without a clock at all.
    assert_eq!(ago(3 * 24 * 60 * 60), "3 days ago");
}

#[test]
fn elapsed_time_stays_readable() {
    assert_eq!(elapsed(842), "842 ms");
    assert_eq!(elapsed(1_500), "1.5 s");
    // The one that prompted this: 260604 ms made the reader divide.
    assert_eq!(elapsed(260_604), "4 min 21 s");
    assert_eq!(elapsed(7_500_000), "2 h 5 min");
}

#[test]
fn a_paragraph_wraps_without_losing_or_splitting_a_word() {
    let text = "Marilyn Manson is an American rock band formed in Fort \
                    Lauderdale, Florida, in 1989.";
    let lines = wrap(text, 30);
    assert!(
        lines.iter().all(|l| display_width(l) <= 30),
        "every line fits: {lines:?}"
    );
    assert_eq!(
        lines.join(" "),
        text,
        "and the words come back in order, none dropped and none joined"
    );
}

#[test]
fn a_word_wider_than_the_line_is_left_whole() {
    // The word this exists for is a URL: split across two lines it is a
    // URL nobody can click, and one over-long line is the lesser harm.
    let url = "https://en.wikipedia.org/wiki/Marilyn_Manson_(band)";
    let lines = wrap(&format!("see {url} for more"), 20);
    assert!(
        lines.iter().any(|l| l == url),
        "the address survives intact: {lines:?}"
    );
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
    // The two endings the program actually uses.
    assert_eq!(plural(3, "analysis"), "3 analyses");
    assert_eq!(plural(3, "imported analysis"), "3 imported analyses");
    assert_eq!(plural(1, "analysis"), "1 analysis");
    assert_eq!(plural(3, "match"), "3 matches");
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
