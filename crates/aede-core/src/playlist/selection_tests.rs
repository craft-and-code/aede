use super::*;
fn catalog() -> Catalog {
    crate::model::build(
        vec![crate::model::ScannedFile {
            path: std::env::temp_dir()
                .join("fictional-queue/album/one.flac")
                .to_str()
                .unwrap()
                .into(),
            size: 1,
            mtime: 0,
            tags: crate::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![],
        0,
        &[],
    )
}
#[test]
fn ordered_relative_absolute_and_repeated_entries_resolve_without_opening_audio() {
    let c = catalog();
    let parent = Path::new(&c.files[0].path).parent().unwrap();
    let text = format!(
        "\u{feff}#EXTM3U\r\n#EXTINF:1,Example\r\none.flac\r\n./one.flac\r\n{}\r\n",
        c.files[0].path
    );
    assert_eq!(resolve(&c, &text, parent).unwrap(), vec![0, 0, 0]);
}

#[test]
fn raw_relative_entries_preserve_initial_whitespace_instead_of_selecting_another_track() {
    let base = catalog();
    let parent = Path::new(&base.files[0].path).parent().unwrap();
    let scanned: Vec<_> = ["one.flac", " one.flac", "\u{3000}one.flac", " #one.flac"]
        .into_iter()
        .map(|name| crate::model::ScannedFile {
            path: parent.join(name).to_str().unwrap().into(),
            size: 1,
            mtime: 0,
            tags: crate::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        })
        .collect();
    let c = crate::model::build(scanned, vec![], 0, &[]);
    let content =
        "\u{feff}#EXTM3U\r\n \r\n one.flac\r\n\u{3000}one.flac\r\n #one.flac\r\n one.flac\r\n";
    let resolved = resolve(&c, content, parent).unwrap();
    let paths: Vec<_> = resolved
        .iter()
        .map(|id| Path::new(&c.file(c.track(*id).unwrap().file_id).unwrap().path))
        .collect();
    assert_eq!(
        paths,
        [" one.flac", "\u{3000}one.flac", " #one.flac", " one.flac"].map(|name| parent.join(name))
    );
}

#[test]
fn an_encoding_bom_does_not_consume_initial_bom_characters_from_the_first_filename() {
    for name in [
        "\u{feff}one.flac",
        "\u{feff}\u{feff}one.flac",
        "\u{feff}#one.flac",
    ] {
        let mut c = catalog();
        let parent = Path::new(&c.files[0].path).parent().unwrap().to_owned();
        c.files[0].path = parent.join(name).to_str().unwrap().into();
        let content = format!("\u{feff}{name}\r\n./{name}\r\n");
        assert_eq!(
            crate::playlist::entry_lines(&content).count(),
            2,
            "{name:?}"
        );
        assert_eq!(resolve(&c, &content, &parent).unwrap(), [0, 0], "{name:?}");
    }
}

#[test]
fn unknown_urls_empty_oversized_and_ambiguous_entries_fail_wholly() {
    let mut c = catalog();
    let parent = Path::new(&c.files[0].path).parent().unwrap().to_owned();
    for text in [
        "one.flac\nmissing.flac",
        "https://example.invalid/a.flac",
        "file:///unknown.flac",
        "#EXTM3U",
        "one.flac\0",
        &"one.flac\n".repeat(4097),
    ] {
        assert!(resolve(&c, text, &parent).is_err(), "{text}");
    }
    let mut other = c.tracks[0].clone();
    other.id = 1;
    c.tracks.push(other);
    assert!(resolve(&c, "one.flac", &parent).is_err());
}
