use super::*;
use crate::accounts_test_support::Fixture;

fn identity(fixture: &Fixture, username: &str) -> Identity {
    let mut accounts = fixture.accounts();
    let (key, _) = accounts
        .create_api_key(username, "personal test", 11)
        .unwrap();
    let account = accounts.find(username).unwrap();
    let identity = Identity {
        owner: account.id.clone(),
        username: account.username.clone(),
        role: account.role,
        key_id: key.id,
        epoch: accounts.epoch().into(),
        revision: account.revision(),
    };
    aede_core::accounts::save(
        &accounts,
        &aede_core::accounts::accounts_path(&fixture.0.data_dir),
    )
    .unwrap();
    identity
}

fn id(fixture: &Fixture, kind: EntityKind) -> String {
    opaque_id(
        fixture.0.catalog.try_read().unwrap().as_ref().unwrap(),
        kind,
        0,
    )
    .unwrap()
}

fn run(
    runtime: &tokio::runtime::Runtime,
    fixture: &Fixture,
    identity: &Identity,
    method: &str,
    pairs: &[(&str, &str)],
) -> Result<Value, ProtocolError> {
    runtime.block_on(execute(
        fixture.0.clone(),
        identity.clone(),
        method,
        Parameters::from_pairs(pairs),
    ))
}

fn persisted(fixture: &Fixture) -> UserData {
    user::load(&user::user_path(&fixture.0.data_dir))
        .unwrap()
        .unwrap_or_default()
}

#[test]
fn favourites_and_ratings_use_existing_annotations_and_never_cross_owners() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let alice = identity(&fixture, "alice");
    let bob = identity(&fixture, "bob");
    let song = id(&fixture, EntityKind::Track);
    let album = id(&fixture, EntityKind::Release);
    let artist = id(&fixture, EntityKind::Artist);
    run(
        &runtime,
        &fixture,
        &alice,
        "star",
        &[
            ("id", &song),
            ("id", &song),
            ("albumId", &album),
            ("artistId", &artist),
        ],
    )
    .unwrap();
    run(
        &runtime,
        &fixture,
        &alice,
        "setRating",
        &[("id", &song), ("rating", "5")],
    )
    .unwrap();
    for target in [&album, &artist] {
        run(
            &runtime,
            &fixture,
            &alice,
            "setRating",
            &[("id", target), ("rating", "4")],
        )
        .unwrap();
    }
    let starred = run(&runtime, &fixture, &alice, "getStarred2", &[]).unwrap();
    assert_eq!(starred["starred2"]["song"].as_array().unwrap().len(), 1);
    assert_eq!(starred["starred2"]["song"][0]["userRating"], 5);
    assert!(starred["starred2"]["song"][0]["starred"].is_string());
    assert_eq!(starred["starred2"]["album"].as_array().unwrap().len(), 1);
    assert_eq!(starred["starred2"]["artist"].as_array().unwrap().len(), 1);
    assert_eq!(starred["starred2"]["album"][0]["userRating"], 4);
    assert_eq!(starred["starred2"]["artist"][0]["userRating"], 4);
    assert!(!starred.to_string().contains("/music"));
    assert_eq!(persisted(&fixture).annotations.len(), 3);
    let theirs = run(&runtime, &fixture, &bob, "getStarred2", &[]).unwrap();
    assert!(theirs["starred2"]["song"].as_array().unwrap().is_empty());
    let before = user::to_json(&persisted(&fixture));
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &alice,
            "star",
            &[("id", &song), ("id", "unknown")]
        )
        .unwrap_err()
        .code,
        70
    );
    assert_eq!(user::to_json(&persisted(&fixture)), before);
    run(&runtime, &fixture, &alice, "unstar", &[("id", &song)]).unwrap();
    assert_eq!(
        persisted(&fixture)
            .annotations
            .iter()
            .find(|a| a.target.kind == EntityKind::Track)
            .unwrap()
            .rating,
        Some(5)
    );
    run(
        &runtime,
        &fixture,
        &alice,
        "setRating",
        &[("id", &song), ("rating", "0")],
    )
    .unwrap();
    assert_eq!(persisted(&fixture).annotations.len(), 2);
}

#[test]
fn static_playlist_crud_preserves_repetitions_and_refuses_other_owners() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let alice = identity(&fixture, "alice");
    let bob = identity(&fixture, "bob");
    let song = id(&fixture, EntityKind::Track);
    let created = run(
        &runtime,
        &fixture,
        &alice,
        "createPlaylist",
        &[
            ("name", "Evening"),
            ("songId", &song),
            ("songId", &song),
            ("songId", &song),
        ],
    )
    .unwrap();
    let playlist = created["playlist"]["id"].as_str().unwrap();
    assert_eq!(created["playlist"]["entry"].as_array().unwrap().len(), 3);
    assert_eq!(created["playlist"]["owner"], "alice");
    assert_eq!(created["playlist"]["public"], false);
    assert!(!created.to_string().contains(&alice.owner));
    for method in ["getPlaylist", "deletePlaylist"] {
        assert_eq!(
            run(&runtime, &fixture, &bob, method, &[("id", playlist)])
                .unwrap_err()
                .code,
            70
        );
    }
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &bob,
            "getPlaylists",
            &[("username", "alice")]
        )
        .unwrap_err()
        .code,
        50
    );
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &bob,
            "updatePlaylist",
            &[("playlistId", playlist), ("name", "stolen")]
        )
        .unwrap_err()
        .code,
        70
    );
    run(
        &runtime,
        &fixture,
        &alice,
        "updatePlaylist",
        &[
            ("playlistId", playlist),
            ("name", "Night"),
            ("comment", "private"),
            ("public", "false"),
            ("songIndexToRemove", "2"),
            ("songIndexToRemove", "0"),
            ("songIdToAdd", &song),
        ],
    )
    .unwrap();
    let updated = run(
        &runtime,
        &fixture,
        &alice,
        "getPlaylist",
        &[("id", playlist)],
    )
    .unwrap();
    assert_eq!(updated["playlist"]["songCount"], 2);
    assert_eq!(updated["playlist"]["name"], "Night");
    assert_eq!(updated["playlist"]["comment"], "private");
    let listed = run(&runtime, &fixture, &alice, "getPlaylists", &[]).unwrap();
    assert_eq!(listed["playlists"]["playlist"].as_array().unwrap().len(), 1);
    assert!(listed["playlists"]["playlist"][0].get("entry").is_none());
    run(
        &runtime,
        &fixture,
        &alice,
        "createPlaylist",
        &[("playlistId", playlist), ("songId", &song)],
    )
    .unwrap();
    assert_eq!(persisted(&fixture).playlists[0].tracks.len(), 1);
    run(
        &runtime,
        &fixture,
        &alice,
        "deletePlaylist",
        &[("id", playlist)],
    )
    .unwrap();
    assert!(persisted(&fixture).playlists.is_empty());
}

#[test]
fn invalid_personal_arguments_never_write_or_discard_playlist_entries() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let alice = identity(&fixture, "alice");
    let song = id(&fixture, EntityKind::Track);
    let created = run(
        &runtime,
        &fixture,
        &alice,
        "createPlaylist",
        &[("name", "Keep"), ("songId", &song)],
    )
    .unwrap();
    let playlist = created["playlist"]["id"].as_str().unwrap();
    let before = user::to_json(&persisted(&fixture));
    for pairs in [
        vec![("playlistId", playlist), ("public", "true")],
        vec![("playlistId", playlist), ("name", " ")],
        vec![("playlistId", playlist), ("songIndexToRemove", "1")],
        vec![
            ("playlistId", playlist),
            ("songIndexToRemove", "0"),
            ("songIndexToRemove", "0"),
        ],
        vec![("playlistId", playlist), ("songIdToAdd", "unknown")],
    ] {
        assert!(run(&runtime, &fixture, &alice, "updatePlaylist", &pairs).is_err());
        assert_eq!(user::to_json(&persisted(&fixture)), before);
    }
    for pairs in [
        vec![("id", song.as_str()), ("rating", "6")],
        vec![("id", song.as_str()), ("rating", "1.5")],
        vec![("id", song.as_str())],
    ] {
        assert_eq!(
            run(&runtime, &fixture, &alice, "setRating", &pairs)
                .unwrap_err()
                .code,
            10
        );
    }
    assert_eq!(
        run(&runtime, &fixture, &alice, "star", &[])
            .unwrap_err()
            .code,
        10
    );
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &alice,
            "getStarred2",
            &[("musicFolderId", "2")]
        )
        .unwrap_err()
        .code,
        70
    );
    assert!(
        run(
            &runtime,
            &fixture,
            &alice,
            "getPlaylists",
            &[("arbitrary", "x")]
        )
        .is_err()
    );
    let mut data = persisted(&fixture);
    data.playlists[0].tracks.push(EntityRef::new(
        EntityKind::Track,
        "/temporarily/unavailable.flac",
    ));
    user::save(&data, &user::user_path(&fixture.0.data_dir)).unwrap();
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &alice,
            "getPlaylist",
            &[("id", playlist)]
        )
        .unwrap_err()
        .code,
        70
    );
    assert_eq!(persisted(&fixture).playlists[0].tracks.len(), 2);
}

#[test]
fn scrobble_batches_persist_only_client_declarations_with_private_counts() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let alice = identity(&fixture, "alice");
    let song = id(&fixture, EntityKind::Track);
    run(
        &runtime,
        &fixture,
        &alice,
        "scrobble",
        &[
            ("id", &song),
            ("id", &song),
            ("time", "1234"),
            ("time", "1234"),
        ],
    )
    .unwrap();
    let data = persisted(&fixture);
    assert!(data.plays.is_empty());
    assert_eq!(data.scrobbles.len(), 2);
    assert_eq!(data.scrobbles[0].at_ms, 1234);
    assert_eq!(data.counts[0].count, 2);
    assert_eq!(data.counts[0].last_played, 1);
    assert_eq!(data.scrobbles[0].owner, alice.owner);
    let before = user::to_json(&data);
    for pairs in [
        vec![
            ("id", song.as_str()),
            ("id", song.as_str()),
            ("time", "1000"),
        ],
        vec![("id", song.as_str()), ("time", "184467440737095516160")],
        vec![("id", song.as_str()), ("time", "253402300799999")],
        vec![("id", song.as_str()), ("time", "-1")],
        vec![("id", song.as_str()), ("submission", "maybe")],
    ] {
        assert_eq!(
            run(&runtime, &fixture, &alice, "scrobble", &pairs)
                .unwrap_err()
                .code,
            10
        );
        assert_eq!(user::to_json(&persisted(&fixture)), before);
    }
}

#[test]
fn catalogue_views_expose_only_the_authenticated_owners_ratings_favourites_and_counts() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let alice = identity(&fixture, "alice");
    let bob = identity(&fixture, "bob");
    let auditor = identity(&fixture, "auditor");
    let song = id(&fixture, EntityKind::Track);
    let album = id(&fixture, EntityKind::Release);
    let artist = id(&fixture, EntityKind::Artist);
    for (target, rating) in [(&song, "5"), (&album, "3"), (&artist, "4")] {
        run(
            &runtime,
            &fixture,
            &alice,
            "setRating",
            &[("id", target), ("rating", rating)],
        )
        .unwrap();
    }
    run(&runtime, &fixture, &alice, "star", &[("albumId", &album)]).unwrap();
    run(&runtime, &fixture, &alice, "scrobble", &[("id", &song)]).unwrap();
    let before = user::to_json(&persisted(&fixture));
    let read = |identity: &Identity, method: &str, pairs: &[(&str, &str)]| {
        runtime
            .block_on(catalog(
                fixture.0.clone(),
                identity.clone(),
                method,
                Parameters::from_pairs(pairs),
            ))
            .unwrap()
    };
    let value = read(&alice, "getSong", &[("id", &song)]);
    assert_eq!(value["song"]["userRating"], 5);
    assert_eq!(value["song"]["playCount"], 1);
    assert!(value["song"].get("starred").is_none());
    let value = read(&alice, "getAlbum", &[("id", &album)]);
    assert_eq!(value["album"]["userRating"], 3);
    assert!(value["album"]["starred"].is_string());
    assert_eq!(value["album"]["song"][0]["userRating"], 5);
    assert_eq!(value["album"]["song"][0]["playCount"], 1);
    assert_eq!(
        read(&alice, "getArtist", &[("id", &artist)])["artist"]["userRating"],
        4
    );
    let searched = read(&alice, "search3", &[("query", "")]);
    assert_eq!(searched["searchResult3"]["song"][0]["userRating"], 5);
    for other in [&bob, &auditor] {
        let value = read(other, "getAlbum", &[("id", &album)]);
        assert!(value["album"].get("starred").is_none());
        assert!(value["album"].get("userRating").is_none());
        assert!(value["album"]["song"][0].get("userRating").is_none());
        assert_eq!(value["album"]["song"][0]["playCount"], 0);
    }
    assert_eq!(user::to_json(&persisted(&fixture)), before);
}

#[test]
fn auditors_revoked_keys_store_failures_and_worker_limits_cannot_mutate_data() {
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let alice = identity(&fixture, "alice");
    let auditor = identity(&fixture, "auditor");
    for method in [
        "star",
        "unstar",
        "setRating",
        "createPlaylist",
        "updatePlaylist",
        "deletePlaylist",
        "scrobble",
    ] {
        assert_eq!(
            run(&runtime, &fixture, &auditor, method, &[])
                .unwrap_err()
                .code,
            50
        );
    }
    run(&runtime, &fixture, &auditor, "getPlaylists", &[]).unwrap();
    let one = fixture
        .0
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .unwrap();
    let two = fixture
        .0
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .unwrap();
    assert_eq!(
        run(&runtime, &fixture, &alice, "getPlaylists", &[])
            .unwrap_err()
            .code,
        0
    );
    drop(one);
    drop(two);
    let lock = StoreLock::try_acquire(&fixture.0.data_dir).unwrap();
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &alice,
            "createPlaylist",
            &[("name", "Busy")]
        )
        .unwrap_err()
        .code,
        0
    );
    drop(lock);
    let mut accounts = fixture.accounts();
    accounts.revoke_api_key("alice", &alice.key_id).unwrap();
    aede_core::accounts::save(
        &accounts,
        &aede_core::accounts::accounts_path(&fixture.0.data_dir),
    )
    .unwrap();
    assert_eq!(
        run(
            &runtime,
            &fixture,
            &alice,
            "createPlaylist",
            &[("name", "Revoked")]
        )
        .unwrap_err()
        .code,
        44
    );
    assert!(!user::user_path(&fixture.0.data_dir).exists());
    std::fs::write(user::user_path(&fixture.0.data_dir), "broken").unwrap();
    assert_eq!(
        run(&runtime, &fixture, &auditor, "getPlaylists", &[])
            .unwrap_err()
            .code,
        0
    );
    assert_eq!(
        std::fs::read_to_string(user::user_path(&fixture.0.data_dir)).unwrap(),
        "broken"
    );
}
