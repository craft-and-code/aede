use super::*;

#[test]
fn revoked_m3u_publication_preserves_the_store_and_removes_an_uncommitted_new_file() {
    use crate::accounts_test_support::Fixture;
    let fixture = Fixture::new();
    let accounts = fixture.accounts();
    let alice = accounts.find("alice").unwrap();
    let catalog = fixture.0.catalog.try_read().unwrap().clone().unwrap();
    let track = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let mut original = UserData::default();
    original
        .entry(&accounts.find("bob").unwrap().id, &track, 1)
        .note = Some("Bob intact".into());
    let path = user::user_path(&fixture.0.data_dir);
    user::save(&original, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    for revoke_at in [1, 2] {
        let (principal, _) = auth::start_session(&fixture.0, &accounts, alice).unwrap();
        let owner = PersonalOwner {
            id: alice.id.clone(),
            principal: Some(principal.clone()),
        };
        let calls = std::cell::Cell::new(0);
        let mut data = original.clone();
        let result = save_selection(
            &catalog,
            &mut data,
            &fixture.0.data_dir,
            &owner.id,
            "Private",
            vec![track.clone()],
            || {
                calls.set(calls.get() + 1);
                if calls.get() == revoke_at {
                    auth::end_session(&fixture.0, &principal)?;
                }
                owner.recheck(&fixture.0)
            },
        );
        assert_eq!(result.err().unwrap().status, StatusCode::UNAUTHORIZED);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        for playlist in data.playlists {
            assert!(
                !target_path(&fixture.0.data_dir, &owner.id, &playlist.id)
                    .unwrap()
                    .exists()
            );
        }
    }
}

#[tokio::test]
async fn private_export_search_filters_all_owned_summaries_before_pagination() {
    use crate::accounts_test_support::{Fixture, http, login};
    let fixture = Fixture::new();
    let catalog = fixture.0.catalog.read().await.clone().unwrap();
    let track = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
    let accounts = fixture.accounts();
    let alice = &accounts.find("alice").unwrap().id;
    let bob = &accounts.find("bob").unwrap().id;
    let mut data = UserData::default();
    data.create_playlist(alice, "Wanted", vec![track.clone()], 1)
        .unwrap();
    data.create_playlist(alice, "Wanted", vec![track.clone()], 2)
        .unwrap();
    data.create_playlist(alice, "Other", vec![track.clone()], 3)
        .unwrap();
    data.create_playlist(bob, "Wanted", vec![track], 4).unwrap();
    user::save(&data, &user::user_path(&fixture.0.data_dir)).unwrap();
    let before = std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let token = login(address, "alice");
        let (status, page, _) = http(
            address,
            "GET",
            "/api/me/v1/m3u?q=wanted&limit=1",
            Some(&token),
            "",
        );
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["name"], "Wanted");
        assert_eq!(page["items"][0]["duplicates"], 2);
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap(),
        before
    );
    server.abort();
}

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
        || Ok(()),
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
        || Ok(()),
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
            tracks.clone(),
            || Ok(())
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
            tracks.clone(),
            || Ok(())
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"manual content");
    let other = save_selection(&catalog, &mut data, &root, "bob", "Fixture", tracks, || {
        Ok(())
    })
    .unwrap();
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

#[test]
fn arbitrary_m3u_selection_preserves_occurrences_and_refuses_ambiguous_or_missing_tracks() {
    let state = crate::test_support::sample_state();
    let catalog = state.catalog.blocking_read().clone().unwrap();
    let token = EntityRef::of(&catalog, EntityKind::Track, 0)
        .unwrap()
        .to_token();
    let tokens = vec![token.clone(), token];
    let (_, selected) = selection(&catalog, None, Some(&tokens)).unwrap();
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[0], selected[1]);
    assert!(selection(&catalog, Some("release:album"), Some(&tokens)).is_err());
    assert!(selection(&catalog, None, Some(&[])).is_err());
    assert!(selection(&catalog, None, Some(&["track:missing".into()])).is_err());
}
