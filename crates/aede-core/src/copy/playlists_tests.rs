use super::*;

#[test]
fn sidecars_follow_adapted_and_converted_paths_and_drop_unselected_entries() {
    let mapping = BTreeMap::from([
        (
            "/music/Album:Live/01?.flac".into(),
            PathBuf::from("Album_Live/01_.mp3"),
        ),
        (
            "/music/Other/02.flac".into(),
            PathBuf::from("Other/02.flac"),
        ),
    ]);
    let text = "#EXTM3U\n#EXTINF:1,First\n01?.flac\n#EXTINF:2,Missing\n03.flac\n../Other/02.flac\nhttps://example.invalid/radio\n";
    let (rendered, omitted) = remap(
        text,
        Some(Path::new("/music/Album:Live/list.m3u")),
        Path::new("Album_Live/list.m3u"),
        &mapping,
    )
    .unwrap();
    assert_eq!(
        rendered,
        "#EXTM3U\n#EXTINF:1,First\n01_.mp3\n../Other/02.flac\n"
    );
    assert_eq!(omitted, 2);
}

#[test]
fn windows_relative_playlists_resolve_without_changing_stored_source_paths() {
    let mapping = BTreeMap::from([(
        path_key(r"C:\Music\Album\01.flac"),
        PathBuf::from("Album/01.opus"),
    )]);
    let (rendered, _) = remap(
        ".\\01.flac\n",
        Some(Path::new(r"C:\Music\Album\list.m3u")),
        Path::new("Album/list.m3u"),
        &mapping,
    )
    .unwrap();
    assert_eq!(rendered, "01.opus\n");
}

#[test]
fn playlist_paths_beginning_with_hash_are_not_comments() {
    let mapping = BTreeMap::from([("/music/#first.flac".into(), PathBuf::from("#first.flac"))]);
    let (rendered, _) = remap(
        "/music/#first.flac\n",
        None,
        Path::new("selection.m3u8"),
        &mapping,
    )
    .unwrap();
    assert_eq!(rendered, "./#first.flac\n");
}

#[test]
fn generated_selection_playlist_preserves_track_order_and_real_paths() {
    let mut first = crate::model::ScannedFile {
        path: "/music/Album?/01.flac".into(),
        size: 100,
        mtime: 0,
        tags: crate::tags::RawTags::default(),
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    };
    first.tags.insert("title", "First");
    first.tags.properties.lossless = true;
    let mut second = first.clone();
    second.path = "/music/Album?/02.flac".into();
    second.tags.insert("title", "Second");
    let catalog = crate::model::build(vec![first, second], vec!["/music".into()], 0, &[]);
    let tracks = vec![catalog.tracks[1].id, catalog.tracks[0].id];
    let mut plan = super::super::plan(
        &catalog,
        &tracks,
        &super::super::Recipe {
            extras: super::super::Extras::None,
            restrict_names: true,
            convert: Some(super::super::transcode::Target::Opus),
            ..Default::default()
        },
    );
    assert_eq!(plan.prepare_playlists(&catalog, &tracks).unwrap(), 0);
    let playlist = plan.items.last().unwrap();
    assert_eq!(playlist.relative, Path::new("aede-selection.m3u8"));
    let content = playlist.contents.as_ref().unwrap();
    let paths: Vec<_> = content
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    assert_eq!(paths, vec!["Album_/02.opus", "Album_/01.opus"]);
}

#[test]
fn a_drive_like_unix_filename_is_resolved_relative_to_its_playlist() {
    let mapping = BTreeMap::from([(
        "/music/Album/C:Interlude.flac".into(),
        PathBuf::from("Album/C_Interlude.flac"),
    )]);
    let (rendered, removed) = remap(
        "C:Interlude.flac\n",
        Some(Path::new("/music/Album/list.m3u")),
        Path::new("Album/list.m3u"),
        &mapping,
    )
    .unwrap();
    assert_eq!(rendered, "C_Interlude.flac\n");
    assert_eq!(removed, 0);
}

#[test]
fn adapted_line_break_names_and_titles_have_valid_selection_entries() {
    let mut file = crate::model::ScannedFile {
        path: "/music/Album/First\nSecond.flac".into(),
        size: 100,
        mtime: 0,
        tags: crate::tags::RawTags::default(),
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    };
    file.tags.insert("title", "First\nSecond\rTitle");
    let catalog = crate::model::build(vec![file], vec!["/music".into()], 0, &[]);
    let tracks: Vec<_> = catalog.tracks.iter().map(|track| track.id).collect();
    let mut plan = super::super::plan(
        &catalog,
        &tracks,
        &super::super::Recipe {
            extras: super::super::Extras::None,
            restrict_names: true,
            ..Default::default()
        },
    );
    plan.prepare_playlists(&catalog, &tracks).unwrap();
    let rendered = plan.items.last().unwrap().contents.as_ref().unwrap();
    assert!(rendered.contains("First Second Title\n"));
    assert_eq!(
        rendered
            .lines()
            .filter(|line| !line.starts_with('#'))
            .collect::<Vec<_>>(),
        vec!["Album/First_Second.flac"]
    );
}
