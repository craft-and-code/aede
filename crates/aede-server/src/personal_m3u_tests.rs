use super::*;
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
