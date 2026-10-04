use super::*;

#[test]
fn terminal_metadata_cannot_emit_osc_or_c1_control_instructions() {
    let attack = "name\x1b]52;c;Y2xpcGJvYXJk\x07\r\u{009b}2J";
    let mut table = Table::new(&["Title"]);
    table.push(vec![attack.into()]);
    for shown in [table.render(), section(attack)] {
        assert!(
            !shown.contains("\x1b]52"),
            "clipboard instructions must be literal"
        );
        assert!(!shown.contains('\u{009b}'));
        assert!(!shown.contains('\x07'));
        assert!(!shown.contains('\r'));
        assert!(shown.contains("name"));
    }
    assert_eq!(literal("note\n\ttab\x1b\r"), "note\n\ttab\\u{1b}\\r");
}

#[test]
fn disabling_color_removes_embedded_styles_without_enabling_other_instructions() {
    let styled = "\x1b[31mname\x1b[0m\x1b]52;c;YQ==\x07";
    assert_eq!(
        literal_styled_with_color(styled, false),
        "name\\u{1b}]52;c;YQ==\\u{7}"
    );
    assert_eq!(
        literal_styled_with_color("\x1b[31mname\x1b[0m", true),
        "\x1b[31mname\x1b[0m"
    );
}

struct LoadingOutput {
    bytes: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    flushed: std::sync::mpsc::Sender<()>,
}

impl std::io::Write for LoadingOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.flushed.send(());
        Ok(())
    }
}

#[test]
fn loading_is_visible_during_the_request_and_cleared_after_failure() {
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (flushed, flushes) = std::sync::mpsc::channel();
    let output = LoadingOutput {
        bytes: bytes.clone(),
        flushed,
    };
    let result = with_loading_output(output, true, "Loading...", || {
        // The initial line is already flushed when the request starts.
        flushes.try_recv().unwrap();
        assert!(bytes.lock().unwrap().starts_with(b"\r  | Loading..."));
        flushes
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the indicator should animate while the request waits");
        Err::<(), _>("network unavailable")
    });
    assert_eq!(result, Err("network unavailable"));
    let rendered = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert!(rendered.contains("\r  / Loading..."));
    assert!(rendered.ends_with("\r              \r"));
}

#[test]
fn redirected_loading_keeps_output_clean_and_returns_the_request_result() {
    let mut output = Vec::new();
    let result = with_loading_output(&mut output, false, "Loading...", || 42);
    assert_eq!(result, 42);
    assert!(output.is_empty());
}

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
    assert_eq!(
        elapsed(u128::MAX),
        "94522879700260684295381835397713 h 23 min"
    );
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
    // A long temporary-directory prefix can fill the column before the file
    // name even starts; cutting the tail names nothing.
    let long = "/temporary/cache/with/a/long/generated/directory/name/bad.flac";
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
    assert_eq!(long_duration(u64::MAX), "213503982334 d 14 h 25 min");
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
