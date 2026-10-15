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
