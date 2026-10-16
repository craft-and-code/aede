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
fn remote_urls_cannot_become_selected_local_colon_paths() {
    let mapping = BTreeMap::from([
        (
            "/music/Album/https:/example.invalid/song.flac".into(),
            PathBuf::from("Album/selected-local.flac"),
        ),
        (
            "/music/Album/file:/example.invalid/song.flac".into(),
            PathBuf::from("Album/selected-file-uri-shaped.flac"),
        ),
        (
            "/music/Album/file:track.flac".into(),
            PathBuf::from("Album/file_track.flac"),
        ),
        (
            "/music/Album/C:Interlude.flac".into(),
            PathBuf::from("Album/C_Interlude.flac"),
        ),
    ]);
    let text = "#EXTM3U\n#EXTINF:1,Remote URL\nhttps://example.invalid/song.flac\n\
                #EXTINF:2,File URI\nfile://example.invalid/song.flac\n\
                #EXTINF:3,Literal local path\n./https:/example.invalid/song.flac\n\
                file:track.flac\nC:Interlude.flac\n";
    let (rendered, omitted) = remap(
        text,
        Some(Path::new("/music/Album/list.m3u")),
        Path::new("Album/list.m3u"),
        &mapping,
    )
    .unwrap();
    assert_eq!(omitted, 2);
    assert_eq!(
        rendered,
        "#EXTM3U\n#EXTINF:3,Literal local path\nselected-local.flac\nfile_track.flac\nC_Interlude.flac\n"
    );

    for root in [
        r"C:\Music\Album",
        r"\\nas\music\Album",
        r"\\?\C:\Music\Album",
        r"\\?\UNC\nas\music\Album",
    ] {
        let audio = format!("{root}\\01.flac");
        let source = format!("{root}\\list.m3u");
        let mapping = BTreeMap::from([(path_key(&audio), PathBuf::from("Album/01.opus"))]);
        let (rendered, omitted) = remap(
            &format!("{audio}\n.\\01.flac\n"),
            Some(Path::new(&source)),
            Path::new("Album/list.m3u"),
            &mapping,
        )
        .unwrap();
        assert_eq!(rendered, "01.opus\n01.opus\n", "{root}");
        assert_eq!(omitted, 0, "{root}");
    }
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
fn initial_whitespace_survives_generated_and_remapped_playlists_with_repeated_tracks() {
    let root = std::env::temp_dir().join(format!(
        "aede-copy-playlist-whitespace-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let originals = [
        (" Leading.flac", b"first original".as_slice()),
        ("#Hash.flac", b"second original".as_slice()),
    ];
    let mut scanned = Vec::new();
    for (name, bytes) in originals {
        let path = root.join(name);
        std::fs::write(&path, bytes).unwrap();
        scanned.push(crate::model::ScannedFile {
            path: path.to_str().unwrap().into(),
            size: bytes.len() as u64,
            mtime: 0,
            tags: crate::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let source_text = "#EXTM3U\n Leading.flac\n./#Hash.flac\n Leading.flac\n";
    let source_playlist = root.join("source.m3u8");
    std::fs::write(&source_playlist, source_text).unwrap();
    let catalog = crate::model::build(scanned, vec![root.to_str().unwrap().into()], 0, &[]);
    let track_for = |name: &str| {
        catalog
            .tracks
            .iter()
            .find(|track| Path::new(&catalog.file(track.file_id).unwrap().path) == root.join(name))
            .unwrap()
            .id
    };
    let tracks = vec![
        track_for(" Leading.flac"),
        track_for("#Hash.flac"),
        track_for(" Leading.flac"),
    ];
    let mut plan = super::super::plan(
        &catalog,
        &tracks,
        &super::super::Recipe {
            extras: super::super::Extras::All,
            restrict_names: true,
            ..Default::default()
        },
    );
    assert_eq!(plan.prepare_playlists(&catalog, &tracks).unwrap(), 0);
    let playlists: Vec<_> = plan
        .items
        .iter()
        .filter(|item| item.contents.is_some())
        .collect();
    assert_eq!(
        playlists.len(),
        2,
        "both the source and selection playlist are planned"
    );
    for item in playlists {
        let rendered = item.contents.as_ref().unwrap();
        assert_eq!(
            crate::playlist::selection::resolve(&catalog, rendered, &root).unwrap(),
            tracks,
            "both generated and remapped playlists retain every selected occurrence"
        );
        assert_eq!(
            rendered
                .lines()
                .filter(|line| !line.starts_with('#'))
                .collect::<Vec<_>>(),
            vec!["./ Leading.flac", "./#Hash.flac", "./ Leading.flac"]
        );
    }
    assert_eq!(
        std::fs::read_to_string(source_playlist).unwrap(),
        source_text
    );
    for (name, bytes) in originals {
        assert_eq!(std::fs::read(root.join(name)).unwrap(), bytes);
    }
    std::fs::remove_dir_all(root).unwrap();
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
