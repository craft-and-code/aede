use super::*;

#[test]
fn artists_sharing_a_parent_folder_do_not_overwrite_each_others_playlists() {
    let files = [("One", "A1"), ("One", "A2"), ("Two", "B1"), ("Two", "B2")]
        .into_iter()
        .map(|(artist, album)| aede_core::model::ScannedFile {
            path: format!("/music/Mixed/{album}/01.flac"),
            size: 1,
            mtime: 0,
            tags: aede_core::tags::RawTags {
                fields: [
                    ("album".into(), vec![album.into()]),
                    ("artist".into(), vec![artist.into()]),
                    ("albumartist".into(), vec![artist.into()]),
                ]
                .into(),
                ..Default::default()
            },
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        })
        .collect();
    let catalog = aede_core::model::build(files, vec!["/music".into()], 0, &[]);
    let wanted = render_folders(&catalog, discographies(&catalog, &[]), Style::Simple).unwrap();
    assert_eq!(
        wanted.len(),
        1,
        "one physical folder has only one output path"
    );
    assert_eq!(wanted[0].1.lines().count(), 4);
}

#[test]
fn playlist_write_failures_are_returned_to_the_caller() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sandbox = std::env::temp_dir().join(format!(
        "aede_playlist_error_{}_{nonce}",
        std::process::id()
    ));
    let music = sandbox.join("Album");
    let data = sandbox.join("data");
    std::fs::create_dir_all(&music).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        music.join("01.flac"),
    )
    .unwrap();
    let (catalog, _) = aede_core::scan::scan(
        std::slice::from_ref(&music),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&data)).unwrap();
    std::fs::create_dir(music.join("Album.m3u")).unwrap();
    let args = Args::parse(vec![
        "playlist".into(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
    ]);
    let result = playlist(&args);
    std::fs::remove_dir_all(&sandbox).unwrap();
    assert!(
        result.is_err(),
        "a failed write must give a failing command status"
    );
}

#[test]
fn releases_in_the_same_folder_share_one_complete_playlist() {
    let catalog = aede_core::model::build(
        vec![
            aede_core::model::ScannedFile {
                path: "/music/Album/01.flac".into(),
                size: 1,
                mtime: 1,
                tags: aede_core::tags::RawTags {
                    fields: [("album".into(), vec!["First album".into()])].into(),
                    ..Default::default()
                },
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            },
            aede_core::model::ScannedFile {
                path: "/music/Album/02.flac".into(),
                size: 1,
                mtime: 1,
                tags: aede_core::tags::RawTags {
                    fields: [("album".into(), vec!["Second album".into()])].into(),
                    ..Default::default()
                },
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            },
        ],
        vec!["/music".into()],
        0,
        &[],
    );
    assert_eq!(catalog.releases.len(), 2);
    let wanted = render_folders(&catalog, albums(&catalog, &[]), Style::Simple).unwrap();
    assert_eq!(wanted.len(), 1);
    assert_eq!(
        wanted[0].1.lines().collect::<Vec<_>>(),
        ["01.flac", "02.flac"]
    );
}
