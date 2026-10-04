use super::*;

#[test]
fn a_utf8_bom_does_not_hide_the_first_timestamp_or_offset_header() {
    for (text, position) in [
        ("\u{feff}[00:01]verse", 1_000),
        ("\u{feff}[offset:250]\n[00:01]verse", 1_250),
    ] {
        let expected = vec![Line {
            at_ms: Some(position),
            text: "verse".into(),
        }];
        assert_eq!(parse(text), expected);
        assert_eq!(parse_complete(text, 100).unwrap(), expected);
    }
    assert_eq!(parse("\u{feff}plain words")[0].text, "plain words");
    assert_eq!(parse("words\u{feff}inside")[0].text, "words\u{feff}inside");
}

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
fn an_unrepresentable_timestamp_stays_readable_as_plain_lyrics() {
    for raw in [
        "[9223372036854775807:00]all aboard",
        "[00:9223372036854775807]all aboard",
        "[-9223372036854775808:00]all aboard",
    ] {
        assert_eq!(
            parse(raw),
            vec![Line {
                at_ms: None,
                text: raw.into()
            }]
        );
    }
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
fn an_overexpanded_chorus_stays_literal_without_losing_the_following_line() {
    let raw = format!("{}{}", "[00:01]".repeat(512), "words ".repeat(700));
    let lines = parse(&format!("{raw}\n[00:02]after"));
    assert_eq!(
        lines.len(),
        2,
        "one literal chorus and its following timed line"
    );
    assert_eq!(
        lines,
        vec![
            Line {
                at_ms: None,
                text: raw
            },
            Line {
                at_ms: Some(2000),
                text: "after".into()
            },
        ]
    );
}

#[test]
fn the_expansion_budget_is_shared_and_preserves_later_plain_text() {
    let chorus = format!("{}{}", "[00:01]".repeat(512), "x".repeat(1024));
    let raw = format!("{chorus}\n{chorus}\n{chorus}\nafter");
    let lines = parse(&raw);
    assert!(lines.iter().map(|line| line.text.len()).sum::<usize>() <= 1024 * 1024);
    assert_eq!(
        lines.iter().filter(|line| line.at_ms.is_some()).count(),
        512
    );
    assert_eq!(
        lines[512],
        Line {
            at_ms: None,
            text: chorus.clone()
        }
    );
    assert_eq!(
        lines[513],
        Line {
            at_ms: None,
            text: chorus
        }
    );
    assert_eq!(lines.last().unwrap().text, "after");
}

#[test]
fn large_tags_keep_a_proportional_budget_for_normal_timed_lines() {
    let chorus = format!("{}{}", "[00:01]".repeat(48), "x".repeat(16 * 1024));
    let raw = format!("{}\n{chorus}", "plain".repeat(62 * 1024));
    let lyrics = from_tag("track.flac", &raw).unwrap();
    assert_eq!(lyrics.lines.len(), 49);
    assert_eq!(
        lyrics
            .lines
            .iter()
            .filter(|line| line.at_ms.is_some())
            .count(),
        48
    );
    let bytes = lyrics
        .lines
        .iter()
        .map(|line| line.text.len())
        .sum::<usize>();
    assert!(bytes > 1024 * 1024);
    assert!(bytes <= raw.len() * 4);
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
fn a_final_timed_blank_survives_to_clear_the_previous_verse() {
    let lyrics = from_tag("track.flac", "[00:01]first\n[00:03]\n\n").unwrap();
    assert_eq!(
        lyrics.lines,
        vec![
            Line {
                at_ms: Some(1_000),
                text: "first".into(),
            },
            Line {
                at_ms: Some(3_000),
                text: String::new(),
            },
        ]
    );
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
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("aede_lyrics_read_{}_{nonce}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
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

    // Lossy decoding can expand Latin-1 bytes; the sidecar still has a fixed
    // expansion budget based on the bounded disk input.
    let expanded = dir.join("expanded.lrc");
    let mut bytes = vec![0xFF; 128 * 1024];
    bytes.push(b'\n');
    let chorus = format!("{}{}", "[00:01]".repeat(32), "x".repeat(32 * 1024));
    bytes.extend_from_slice(chorus.as_bytes());
    std::fs::write(&expanded, bytes).unwrap();
    let lyrics = read(&expanded).expect("lyrics");
    assert!(!lyrics.synced());
    assert_eq!(lyrics.lines.len(), 2);
    assert_eq!(lyrics.lines[1].text, chorus);
    assert!(
        lyrics
            .lines
            .iter()
            .map(|line| line.text.len())
            .sum::<usize>()
            <= 1024 * 1024
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sidecar_errors_after_the_read_limit_do_not_hide_the_lyrics_prefix() {
    struct UnreadableTail;
    impl std::io::Read for UnreadableTail {
        fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("must not read past the lyrics limit"))
        }
    }
    let prefix = "x".repeat(LIMIT);
    let reader = std::io::Cursor::new(prefix.as_bytes()).chain(UnreadableTail);
    let text = read_text(reader).expect("the requested lyrics prefix is fully readable");
    assert_eq!(text, prefix);
}

#[test]
fn complete_parsing_refuses_expansion_without_degrading_timed_lyrics() {
    let raw = "[00:01][00:02]chorus\n[00:03]verse";
    assert_eq!(parse_complete(raw, 16), Err(ParseLimitExceeded));
    let complete = parse_complete(raw, 17).unwrap();
    assert_eq!(complete, parse(raw));
    assert_eq!(complete.len(), 3);
    assert!(complete.iter().all(|line| line.at_ms.is_some()));
}

#[test]
fn complete_parsing_bounds_plain_and_malformed_text_as_well_as_cues() {
    for raw in ["plain", "[invalid]words", "[00:01]a\nplain"] {
        let expected = parse(raw);
        let budget = expected.iter().map(|line| line.text.len()).sum::<usize>();
        assert_eq!(parse_complete(raw, budget).unwrap(), expected);
        assert_eq!(parse_complete(raw, budget - 1), Err(ParseLimitExceeded));
    }
    assert!(parse_complete("[00:00]\n[ar:artist]", 0).is_ok());
}
