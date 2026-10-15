use super::*;
use crate::user::{Play, Scrobble};
fn library() -> Catalog {
    let files = ["/music/Album/01.flac", "/music/Album/02.flac"]
        .iter()
        .map(|path| {
            crate::model::tests::track(path, &[("album", "Example"), ("artist", "Fictional")], 1000)
        })
        .collect();
    crate::model::build(files, vec!["/music".into()], 0, &[])
}
#[test]
fn every_track_must_be_completed_by_the_same_owner_across_sessions() {
    let catalog = library();
    let mut data = UserData::default();
    for (index, track) in catalog.tracks.iter().enumerate() {
        let reference = EntityRef::of(&catalog, EntityKind::Track, track.id).unwrap();
        data.record_play(Play {
            owner: "alice".into(),
            track: reference,
            at: index as u64 + 1,
            ms_played: 1000,
            completed: true,
        });
        assert_eq!(
            completed_albums(&catalog, &data, "alice").len(),
            usize::from(index == 1)
        );
    }
    assert!(completed_albums(&catalog, &data, "bob").is_empty());
    data.forget_history("alice");
    assert!(completed_albums(&catalog, &data, "alice").is_empty());
}
#[test]
fn partial_visits_declared_scrobbles_and_empty_editions_cannot_complete_an_album() {
    let catalog = library();
    let mut data = UserData::default();
    for track in &catalog.tracks {
        let reference = EntityRef::of(&catalog, EntityKind::Track, track.id).unwrap();
        data.record_play(Play {
            owner: "alice".into(),
            track: reference.clone(),
            at: 1,
            ms_played: 100,
            completed: false,
        });
        data.record_scrobble(Scrobble {
            owner: "alice".into(),
            track: reference,
            at_ms: 1000,
        })
        .unwrap();
    }
    assert!(completed_albums(&catalog, &data, "alice").is_empty());
    let mut empty = catalog.clone();
    empty.releases[0].track_ids.clear();
    assert!(completed_albums(&empty, &data, "alice").is_empty());
}
