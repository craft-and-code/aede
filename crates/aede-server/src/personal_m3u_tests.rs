use super::*;
#[test]
fn summaries_group_exact_legacy_duplicates_without_erasing_or_merging_different_lists() {
    let state = crate::test_support::sample_state();
    let catalog = state.catalog.blocking_read().clone().unwrap();
    let track = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut data = UserData::default();
    for now in 1..=4 {
        data.create_playlist("alice", "Fixture", vec![track.clone()], now)
            .unwrap();
    }
    data.create_playlist("alice", "Fixture", vec![track.clone(), track.clone()], 5)
        .unwrap();
    data.create_playlist("alice", "Other", vec![track.clone()], 6)
        .unwrap();
    data.create_playlist("bob", "Fixture", vec![track], 7)
        .unwrap();
    let rows = unique_summaries(&data, "alice");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .find(|r| r.name == "Fixture" && r.track_count == 1)
            .unwrap()
            .duplicates,
        4
    );
    assert_eq!(data.playlists.len(), 7);
}
#[test]
fn private_m3u_paths_cannot_be_selected_by_names_or_escape_the_store() {
    let root = Path::new("/private/test-data");
    assert!(target_path(root, "../other", "playlist-test").is_err());
    assert!(target_path(root, "alice", "../private").is_err());
    assert_eq!(
        target_path(root, "alice", "playlist-test").unwrap(),
        root.join("playlists/alice/playlist-test.m3u")
    );
}
#[test]
fn m3u_preserves_track_order_and_refuses_missing_entries() {
    let state = crate::test_support::sample_state();
    let catalog = state.catalog.blocking_read().clone().unwrap();
    let track = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut data = UserData::default();
    let id = data
        .create_playlist("alice", "Fixture", vec![track.clone(), track], 1)
        .unwrap();
    let mut playlist = data.playlist("alice", &id).unwrap().clone();
    let answer = published(&catalog, &playlist).unwrap();
    assert_eq!(answer.content.matches("/music/album/01.flac").count(), 2);
    playlist.tracks[0] = EntityRef {
        kind: EntityKind::Track,
        key: "missing".into(),
    };
    assert!(published(&catalog, &playlist).is_err());
}

#[test]
fn repeated_publication_restores_a_missing_file_without_duplicate_playlists() {
    let root = std::env::temp_dir().join(format!("aede_m3u_restore_{}", std::process::id()));
    let state = crate::test_support::sample_state();
    let catalog = state.catalog.blocking_read().clone().unwrap();
    let tracks = vec![EntityRef::of(&catalog, EntityKind::Track, 0).unwrap()];
    let mut data = UserData::default();
    let first = save_selection(
        &catalog,
        &mut data,
        &root,
        "alice",
        "Fixture",
        tracks.clone(),
    )
    .unwrap();
    let path = target_path(&root, "alice", &first.id).unwrap();
    let before = std::fs::read(user::user_path(&root)).unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored = save_selection(
        &catalog,
        &mut data,
        &root,
        "alice",
        "Fixture",
        tracks.clone(),
    )
    .unwrap();
    assert_eq!(restored.id, first.id);
    assert_eq!(std::fs::read(&path).unwrap(), first.content.as_bytes());
    assert_eq!(data.playlists.len(), 1);
    assert_eq!(std::fs::read(user::user_path(&root)).unwrap(), before);
    assert_eq!(
        save_selection(
            &catalog,
            &mut data,
            &root,
            "alice",
            "Fixture",
            tracks.clone()
        )
        .unwrap()
        .id,
        first.id
    );
    std::fs::write(&path, b"manual content").unwrap();
    assert!(
        save_selection(
            &catalog,
            &mut data,
            &root,
            "alice",
            "Fixture",
            tracks.clone()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"manual content");
    let other = save_selection(&catalog, &mut data, &root, "bob", "Fixture", tracks).unwrap();
    assert_ne!(other.id, first.id);
    assert_eq!(data.playlists.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_library_publication_recreates_album_and_complete_artist_files() {
    let root = std::env::temp_dir().join(format!("aede_library_m3u_{}", std::process::id()));
    let album = root.join("music/Artist/First");
    let second = root.join("music/Artist/Second");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let state = crate::test_support::sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.roots = vec![root.join("music").to_string_lossy().into()];
    catalog.releases[0].folder = album.to_string_lossy().into();
    catalog.releases[0].year = Some(2000);
    catalog.files[0].path = album.join("01.flac").to_string_lossy().into();
    std::fs::write(&catalog.files[0].path, b"unchanged audio fixture").unwrap();
    let mut release = catalog.releases[0].clone();
    release.id = 1;
    release.folder = second.to_string_lossy().into();
    release.year = Some(2001);
    release.track_ids = vec![1];
    let mut track = catalog.tracks[0].clone();
    track.id = 1;
    track.file_id = 1;
    track.release_id = Some(1);
    let mut file = catalog.files[0].clone();
    file.id = 1;
    file.path = second.join("02.flac").to_string_lossy().into();
    std::fs::write(&file.path, b"second audio fixture").unwrap();
    catalog.releases.push(release);
    catalog.tracks.push(track);
    catalog.files.push(file);
    let outputs = publish_library(&catalog, 0).unwrap();
    assert_eq!(outputs.len(), 2);
    let album_file = album.join("First.m3u");
    let artist_file = album.parent().unwrap().join("Artist.m3u");
    assert!(
        std::fs::read_to_string(&album_file)
            .unwrap()
            .contains("\n01.flac\n")
    );
    let artist_text = std::fs::read_to_string(&artist_file).unwrap();
    assert!(artist_text.contains("First/01.flac"));
    assert!(artist_text.contains("Second/02.flac"));
    std::fs::remove_file(&album_file).unwrap();
    std::fs::remove_file(&artist_file).unwrap();
    publish_library(&catalog, 0).unwrap();
    assert_eq!(std::fs::read_to_string(&artist_file).unwrap(), artist_text);
    assert_eq!(
        std::fs::read(&catalog.files[0].path).unwrap(),
        b"unchanged audio fixture"
    );
    catalog.roots = vec![root.join("outside").to_string_lossy().into()];
    assert!(publish_library(&catalog, 0).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
