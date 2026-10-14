use super::*;
use crate::user::Annotation;
use crate::{model::EntityKind, user::EntityRef};
#[test]
fn locked_successes_are_opaque_and_owner_unlocks_survive_removed_annotations_and_history() {
    let catalog = Catalog::default();
    let mut data = UserData::default();
    let initial = refresh(&catalog, &mut data, "alice", false, 10).unwrap();
    assert!(initial["total"].as_u64().unwrap() >= 50);
    assert!(
        initial["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("title").is_none() && r.get("description").is_none())
    );
    data.annotations.push(Annotation {
        owner: "alice".into(),
        loved: true,
        rating: Some(1),
        note: Some("Original thought".into()),
        ..Default::default()
    });
    let alice = refresh(&catalog, &mut data, "alice", true, 20).unwrap();
    assert_eq!(alice["unlocked"], 4);
    let bob = refresh(&catalog, &mut data, "bob", false, 30).unwrap();
    assert_eq!(bob["unlocked"], 0);
    data.annotations.clear();
    data.plays.clear();
    let again = refresh(&catalog, &mut data, "alice", false, 40).unwrap();
    assert_eq!(again["unlocked"], 4);
    assert!(
        again["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["unlocked_at"].as_u64())
            .all(|at| at == 20)
    );
}
#[test]
fn corrupt_private_successes_are_retained_and_partial_audio_is_not_completion() {
    let catalog = Catalog::default();
    let mut data = UserData::default();
    data.plays.push(crate::user::Play {
        owner: "alice".into(),
        track: EntityRef::new(EntityKind::Track, "/music/one.wav"),
        at: 1,
        ms_played: 100,
        completed: false,
    });
    assert_eq!(
        refresh(&catalog, &mut data, "alice", false, 20).unwrap()["unlocked"],
        0
    );
    data.client_preferences[0].value = "broken".into();
    assert!(refresh(&catalog, &mut data, "alice", true, 30).is_err());
    assert_eq!(data.client_preferences[0].value, "broken");
}

#[test]
fn full_multidisc_album_and_principal_discography_require_every_owner_completed_track() {
    use crate::model::{Artist, AudioFile, Release, Track};
    let catalog = Catalog {
        artists: vec![Artist {
            id: 0,
            name: "Principal".into(),
            key: "principal".into(),
            ..Default::default()
        }],
        files: (0..3)
            .map(|id| AudioFile {
                id,
                path: format!("/music/{id}.wav"),
                ..Default::default()
            })
            .collect(),
        tracks: (0..3)
            .map(|id| Track {
                id,
                file_id: id,
                disc_no: Some(if id == 0 { 1 } else { 2 }),
                ..Default::default()
            })
            .collect(),
        releases: vec![
            Release {
                id: 0,
                album_artist_id: Some(0),
                track_ids: vec![0, 1],
                ..Default::default()
            },
            Release {
                id: 1,
                album_artist_id: Some(0),
                track_ids: vec![2],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut data = UserData::default();
    let play = |owner: &str, id| crate::user::Play {
        owner: owner.into(),
        track: EntityRef::of(&catalog, EntityKind::Track, id).unwrap(),
        at: 1,
        ms_played: 100,
        completed: true,
    };
    data.plays.push(play("alice", 0));
    data.plays.push(play("bob", 1));
    let before = refresh(&catalog, &mut data, "alice", false, 10).unwrap();
    assert!(
        !before["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["icon"] == "disc" || r["icon"] == "users")
    );
    data.plays.push(play("alice", 1));
    let album = refresh(&catalog, &mut data, "alice", false, 20).unwrap();
    assert!(
        album["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["icon"] == "disc")
    );
    assert!(
        !album["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["icon"] == "users")
    );
    data.plays.push(play("alice", 2));
    let all = refresh(&catalog, &mut data, "alice", false, 30).unwrap();
    assert!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["icon"] == "users")
    );
}
