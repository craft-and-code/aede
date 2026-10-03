use super::super::test_support::library;
use super::*;

fn track(path: &str) -> EntityRef {
    EntityRef::new(EntityKind::Track, path)
}

#[test]
fn static_playlists_preserve_order_repetitions_and_owner_isolation() {
    let mut data = UserData::default();
    let songs = vec![track("/m/b.flac"), track("/m/a.flac"), track("/m/b.flac")];
    let id = data
        .create_playlist("alice", "My playlist", songs.clone(), 10)
        .unwrap();
    assert!(data.playlist("bob", &id).is_none());
    assert!(!data.delete_playlist("bob", &id));
    assert!(
        data.update_playlist("bob", &id, Some("stolen"), None, None, 20)
            .is_err()
    );
    data.update_playlist("alice", &id, None, Some("private description"), None, 20)
        .unwrap();
    let restored = from_json(&to_json(&data)).unwrap();
    assert_eq!(restored.playlist("alice", &id).unwrap().tracks, songs);
    assert_eq!(restored.playlists, data.playlists);
    assert!(data.delete_playlist("alice", &id));
    assert!(data.playlists.is_empty());
}

#[test]
fn playlist_invalid_updates_and_limits_do_not_change_existing_data() {
    let mut data = UserData::default();
    let id = data
        .create_playlist("alice", "Valid", vec![track("/m/a")], 10)
        .unwrap();
    let before = to_json(&data);
    for name in [" ".to_string(), "x".repeat(257), "bad\0name".to_string()] {
        assert!(
            data.update_playlist("alice", &id, Some(&name), None, None, 11)
                .is_err()
        );
        assert_eq!(to_json(&data), before);
    }
    assert!(
        data.create_playlist(
            "alice",
            "Too large",
            vec![track("/m/a"); PLAYLIST_TRACK_LIMIT + 1],
            11
        )
        .is_err()
    );
    for _ in 1..PLAYLIST_OWNER_LIMIT {
        data.create_playlist("alice", "Same name is fine", vec![], 11)
            .unwrap();
    }
    assert!(
        data.create_playlist("alice", "One too many", vec![], 11)
            .is_err()
    );
    assert!(
        data.create_playlist("bob", "Different owner", vec![], 11)
            .is_ok()
    );
    for owner in ["bob", "carol", "dan"] {
        while data
            .playlists
            .iter()
            .filter(|playlist| playlist.owner == owner)
            .count()
            < PLAYLIST_OWNER_LIMIT
        {
            data.create_playlist(owner, "Bounded", vec![], 11).unwrap();
        }
    }
    assert_eq!(data.playlists.len(), PLAYLIST_LIMIT);
    assert!(
        data.create_playlist("eve", "Global limit", vec![], 11)
            .is_err()
    );
}

#[test]
fn native_history_removal_preserves_declared_listen_counts_and_last_time() {
    let song = track("/m/a.flac");
    let mut data = UserData::default();
    data.record_play(Play {
        owner: "alice".into(),
        track: song.clone(),
        at: 1,
        ms_played: 50,
        completed: false,
    });
    data.record_scrobble(Scrobble {
        owner: "alice".into(),
        track: song.clone(),
        at_ms: 2000,
    })
    .unwrap();
    assert!(data.forget_last_play("alice", &song));
    assert_eq!(data.play_count("alice", &song), 1);
    assert_eq!(data.counts[0].last_played, 2);
    assert_eq!(data.scrobbles.len(), 1);
}

#[test]
fn merged_playlist_limits_fail_save_before_replacing_recoverable_file() {
    let root = std::env::temp_dir().join(format!(
        "aede-playlist-limit-{}-{}",
        std::process::id(),
        crate::accounts::random_token().unwrap()
    ));
    let path = root.join("user.json");
    let mut data = UserData::default();
    data.create_playlist("alice", "Keep", vec![track("/m/a")], 1)
        .unwrap();
    save(&data, &path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let mut imported = UserData::default();
    for _ in 0..PLAYLIST_OWNER_LIMIT {
        imported
            .create_playlist("alice", "Imported", vec![], 1)
            .unwrap();
    }
    super::super::merge(&mut data, imported);
    assert_eq!(data.playlists.len(), PLAYLIST_OWNER_LIMIT + 1);
    assert!(save(&data, &path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn new_private_tables_refuse_malformed_rows_instead_of_discarding_them() {
    let mut data = UserData::default();
    data.create_playlist("alice", "Keep", vec![track("/m/a")], 1)
        .unwrap();
    data.record_scrobble(Scrobble {
        owner: "alice".into(),
        track: track("/m/a"),
        at_ms: 1234,
    })
    .unwrap();
    let document = to_json(&data);
    for table in ["playlists", "scrobbles"] {
        let mut malformed = document.clone();
        malformed.set(table, Json::Arr(vec![Json::obj()]));
        assert!(from_json(&malformed).is_err());
        malformed.set(table, Json::obj());
        assert!(from_json(&malformed).is_err());
    }
    let mut duplicate = document.clone();
    let playlist = document.get("playlists").unwrap().as_arr().unwrap()[0].clone();
    duplicate.set("playlists", Json::Arr(vec![playlist.clone(), playlist]));
    assert!(from_json(&duplicate).is_err());
    let mut bad_track = document;
    let mut row = bad_track.get("scrobbles").unwrap().as_arr().unwrap()[0].clone();
    row.set("track", "artist:alice".into());
    bad_track.set("scrobbles", Json::Arr(vec![row]));
    assert!(from_json(&bad_track).is_err());
    assert!(
        from_json(&to_json(&UserData::default()))
            .unwrap()
            .playlists
            .is_empty()
    );
}

#[test]
fn client_scrobbles_keep_unknown_duration_and_completion_out_of_native_plays() {
    let song = track("/m/a.flac");
    let mut data = UserData::default();
    for at_ms in (1000..1000 + HISTORY_LIMIT as u64).rev() {
        data.record_scrobble(Scrobble {
            owner: "alice".into(),
            track: song.clone(),
            at_ms,
        })
        .unwrap();
    }
    data.record_scrobble(Scrobble {
        owner: "alice".into(),
        track: song.clone(),
        at_ms: 1,
    })
    .unwrap();
    assert!(data.plays.is_empty());
    assert_eq!(data.scrobbles.len(), HISTORY_LIMIT);
    assert_eq!(data.scrobbles[0].at_ms, 1000);
    assert_eq!(data.play_count("alice", &song), HISTORY_LIMIT as u32 + 1);
    assert_eq!(data.counts[0].last_played, 1);
    let back = from_json(&to_json(&data)).unwrap();
    assert_eq!(back.scrobbles, data.scrobbles);
    assert!(back.plays.is_empty());
    assert_eq!(data.forget_history("bob"), (0, 0));
    assert_eq!(data.forget_history("alice"), (HISTORY_LIMIT, 1));
    assert!(data.scrobbles.is_empty());
}

#[test]
fn imports_preserve_scrobble_multiplicity_and_choose_newer_playlist_edits() {
    let mut data = UserData::default();
    let id = data
        .create_playlist("alice", "Older", vec![track("/m/a")], 1)
        .unwrap();
    for _ in 0..2 {
        data.record_scrobble(Scrobble {
            owner: "alice".into(),
            track: track("/m/a"),
            at_ms: 1234,
        })
        .unwrap();
    }
    let mut incoming = data.clone();
    incoming
        .update_playlist("alice", &id, Some("Newer"), None, None, 2)
        .unwrap();
    let mut restored = UserData::default();
    super::super::merge(&mut restored, data);
    super::super::merge(&mut restored, incoming.clone());
    assert_eq!(restored.scrobbles.len(), 2);
    assert_eq!(restored.play_count("alice", &track("/m/a")), 2);
    assert_eq!(restored.playlist("alice", &id).unwrap().name, "Newer");
    let before = to_json(&restored);
    let report = super::super::merge(&mut restored, incoming);
    assert_eq!(report.plays, 0);
    assert_eq!(to_json(&restored), before);
}

#[test]
fn reconciliation_retains_missing_playlist_tracks_and_relocates_proven_identity() {
    let mut data = UserData::default();
    let old = track("/old/a.flac");
    let id = data
        .create_playlist("alice", "Moving", vec![old.clone(), old.clone()], 1)
        .unwrap();
    data.record_scrobble(Scrobble {
        owner: "alice".into(),
        track: old.clone(),
        at_ms: 2000,
    })
    .unwrap();
    reconcile(&mut data, &library(&["/old/a.flac"]));
    let new = library(&["/new/a.flac"]);
    let report = reconcile(&mut data, &new);
    assert!(report.moved >= 3);
    assert_eq!(
        data.playlist("alice", &id).unwrap().tracks,
        vec![track("/new/a.flac"); 2]
    );
    assert_eq!(data.scrobbles[0].track, track("/new/a.flac"));
    reconcile(&mut data, &Catalog::default());
    assert_eq!(data.playlist("alice", &id).unwrap().tracks.len(), 2);
    assert_eq!(data.scrobbles.len(), 1);
}

#[test]
fn explicit_playlist_reattachment_undo_refuses_new_playlist_edits() {
    let mut data = UserData::default();
    let old = track("/old/a.flac");
    let new = track("/new/b.flac");
    let catalog = library(&["/new/b.flac"]);
    let id = data
        .create_playlist("alice", "Moving", vec![old.clone(); 2], 1)
        .unwrap();
    let untouched = data
        .create_playlist("bob", "Other owner", vec![old.clone()], 1)
        .unwrap();
    data.record_scrobble(Scrobble {
        owner: "alice".into(),
        track: old.clone(),
        at_ms: 1000,
    })
    .unwrap();
    let relink_id = data.relink("alice", &old, &new, &catalog, 2).unwrap();
    assert_eq!(
        data.playlist("alice", &id).unwrap().tracks,
        vec![new.clone(); 2]
    );
    assert_eq!(
        data.playlist("bob", &untouched).unwrap().tracks,
        vec![old.clone()]
    );
    let mut restored = from_json(&to_json(&data)).unwrap();
    restored.undo_relink("alice", relink_id, 3).unwrap();
    assert_eq!(
        restored.playlist("alice", &id).unwrap().tracks,
        vec![old.clone(); 2]
    );
    assert_eq!(restored.scrobbles[0].track, old);
    data.update_playlist("alice", &id, Some("Edited later"), None, None, 4)
        .unwrap();
    let before = to_json(&data);
    assert!(data.undo_relink("alice", relink_id, 5).is_err());
    assert_eq!(to_json(&data), before);
}
