use super::*;

#[test]
fn a_plain_text_file_is_lines_with_no_times() {
    let lines = parse("First line\nSecond line\n");
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        Line {
            at_ms: None,
            text: "First line".into()
        }
    );
    assert!(lines.iter().all(|l| l.at_ms.is_none()));
}

#[test]
fn a_timed_line_gives_up_its_moment() {
    // The three spellings that are actually written: seconds, hundredths,
    // milliseconds.
    let lines = parse("[00:12]a\n[00:12.34]b\n[01:02.345]c\n");
    assert_eq!(
        lines.iter().map(|l| l.at_ms).collect::<Vec<_>>(),
        vec![Some(12_000), Some(12_340), Some(62_345)]
    );
    assert_eq!(lines[2].text, "c");
}

#[test]
fn a_chorus_timed_twice_appears_twice() {
    // `[00:12][01:44] the chorus` is one line sung twice, and a reader that
    // kept only the first would leave a player silent at its second turn.
    let lines = parse("[00:12.00][01:44.00]the chorus\n");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].at_ms, Some(12_000));
    assert_eq!(lines[1].at_ms, Some(104_000));
    assert_eq!(lines[0].text, lines[1].text);
}

#[test]
fn the_metadata_headers_are_dropped_and_the_offset_applied() {
    // `[ar:]` and friends repeat what the tags already say, and this file is
    // not where the artist's name is settled. `[offset:]` is different: it
    // exists to shift a timing that was made against another encoding.
    let lines = parse("[ar:Ozzy]\n[ti:Crazy Train]\n[offset:+500]\n[00:10.00]all aboard\n");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].at_ms, Some(10_500));
    let back = parse("[offset:-2000]\n[00:10.00]all aboard\n");
    assert_eq!(back[0].at_ms, Some(8_000));
    // An offset that would send a line before the start clamps there rather
    // than wrapping around into an enormous number.
    let far = parse("[offset:-99000]\n[00:10.00]all aboard\n");
    assert_eq!(far[0].at_ms, Some(0));
}

#[test]
fn a_tag_holding_lrc_is_read_as_lrc() {
    // Plenty of taggers write the synced text straight into `LYRICS`, and a
    // reader that only understood plain text would show a page of
    // `[00:12.34]` to somebody who asked for the words.
    let lyrics = from_tag("/m/a.flac", "[00:01.00]one\n[00:02.00]two").expect("lyrics");
    assert!(lyrics.synced());
    assert_eq!(lyrics.text(), "one\ntwo");
    assert_eq!(lyrics.source, Source::Tag);
}

#[test]
fn a_verse_break_survives_and_the_edges_do_not() {
    let lyrics = from_tag("/m/a.flac", "\n\nfirst\n\nsecond\n\n\n").expect("lyrics");
    assert_eq!(lyrics.text(), "first\n\nsecond");
    assert!(!lyrics.synced());
}

#[test]
fn nothing_at_all_is_nothing_rather_than_empty_lyrics() {
    // A tag holding spaces is a tag holding nothing, and a page announcing
    // "Lyrics" over a blank space is worse than a page with no such
    // section.
    assert!(from_tag("/m/a.flac", "   \n\n  ").is_none());
    assert!(from_tag("/m/a.flac", "").is_none());
}

#[test]
fn a_sidecar_sits_beside_its_track_under_the_same_name() {
    assert_eq!(
        sidecar_of(Path::new("/m/Album/01 So What.flac")),
        Path::new("/m/Album/01 So What.lrc")
    );
    assert!(is_sidecar("01 So What.lrc"));
    assert!(is_sidecar("01 So What.LRC"));
    assert!(!is_sidecar("01 So What.flac"));
    assert!(!is_sidecar("lrc"));
}

#[test]
fn a_sidecar_is_read_bounded_and_a_missing_one_is_no_error() {
    let dir = std::env::temp_dir().join("aede_lyrics_read");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(read(&dir.join("gone.lrc")).is_none());

    let path = dir.join("song.lrc");
    std::fs::write(&path, "[00:01.00]one\n[00:02.00]two\n").unwrap();
    let lyrics = read(&path).expect("lyrics");
    assert_eq!(lyrics.source, Source::Sidecar);
    assert_eq!(lyrics.origin, path.to_string_lossy());
    assert!(lyrics.synced());

    // Bytes that are not UTF-8 are replaced, not refused: a .lrc written on
    // a Windows machine in 2003 is Latin-1 as often as not.
    let latin = dir.join("latin.lrc");
    std::fs::write(&latin, [b'c', b'a', b'f', 0xE9]).unwrap();
    assert!(read(&latin).is_some());

    // And a file nobody should have called lyrics is not read whole.
    let huge = dir.join("huge.lrc");
    std::fs::write(&huge, "x\n".repeat(LIMIT)).unwrap();
    let lyrics = read(&huge).expect("lyrics");
    assert!(lyrics.text().len() <= LIMIT, "{}", lyrics.text().len());
    let _ = std::fs::remove_dir_all(&dir);
}
