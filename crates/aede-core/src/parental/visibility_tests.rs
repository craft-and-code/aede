use super::*;
use crate::model::{self, Credit, EntityKind};
use crate::user::EntityRef;
fn catalogue() -> Catalog {
    use crate::model::tests::track;
    model::build(
        vec![
            track(
                "/library/Blue/01.flac",
                &[
                    ("artist", "Blue Group"),
                    ("albumartist", "Blue Group"),
                    ("album", "Blue"),
                    ("title", "Blue First"),
                    ("genre", "Rock"),
                    ("label", "Blue Label"),
                ],
                1000,
            ),
            track(
                "/library/Gold/01.flac",
                &[
                    ("artist", "Gold Group"),
                    ("albumartist", "Gold Group"),
                    ("album", "Gold"),
                    ("title", "Gold First"),
                    ("genre", "Jazz"),
                    ("label", "Gold Label"),
                ],
                2000,
            ),
            track(
                "/library/Mix/01.flac",
                &[
                    ("artist", "Gold Group"),
                    ("album", "Mix"),
                    ("title", "Gold Guest"),
                    ("compilation", "1"),
                    ("musicbrainz_trackid", "shared-recording"),
                ],
                3000,
            ),
            track(
                "/library/Blue/02.flac",
                &[
                    ("artist", "Blue Group"),
                    ("albumartist", "Blue Group"),
                    ("album", "Blue"),
                    ("title", "Shared"),
                    ("musicbrainz_trackid", "shared-recording"),
                ],
                3000,
            ),
        ],
        vec!["/library".into()],
        100,
        &[],
    )
}
#[test]
fn blocked_groups_and_shared_recordings_disappear_without_changing_visible_keys() {
    let source = catalogue();
    let visible = filtered_catalog(&source, &["artist:blue group".into()]).unwrap();
    assert_eq!(visible.tracks.len(), 1);
    assert_eq!(
        visible
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Gold Group"]
    );
    assert_eq!(visible.files[0].path, "/library/Gold/01.flac");
    assert_eq!(visible.files[0].id, 0);
    assert_eq!(visible.tracks[0].file_id, 0);
    assert_eq!(source.tracks.len(), 4);
    for kind in [
        EntityKind::Artist,
        EntityKind::Release,
        EntityKind::Track,
        EntityKind::Recording,
        EntityKind::Label,
        EntityKind::Genre,
    ] {
        assert!(
            EntityRef::of(&visible, kind, 0)
                .unwrap()
                .resolve(&source)
                .is_some()
        );
    }
    assert!(allows_reference(&visible, "track:/library/Gold/01.flac"));
    assert!(!allows_reference(&visible, "track:/library/Blue/01.flac"));
    assert!(!allows_reference(&visible, "artist:blue group"));
    assert!(!allows_reference(&visible, "track:"));
    assert_eq!(visible.releases[0].track_ids, vec![0]);
    assert!(visible.relations.iter().all(|relation| {
        EntityRef::of(&visible, relation.source_kind, relation.source_id).is_some()
            && EntityRef::of(&visible, relation.target_kind, relation.target_id).is_some()
    }));
}
#[test]
fn nonperforming_recording_and_work_credits_also_block_the_audio() {
    let mut source = catalogue();
    let blue = EntityRef::parse_token("artist:blue group")
        .unwrap()
        .resolve(&source)
        .unwrap();
    let gold = EntityRef::parse_token("track:/library/Gold/01.flac")
        .unwrap()
        .resolve(&source)
        .unwrap();
    source.credits.push(Credit {
        artist_id: blue,
        entity_kind: EntityKind::Recording,
        entity_id: source.track(gold).unwrap().recording_id,
        role: "composer".into(),
        credited_as: None,
        attributes: vec![],
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    });
    let visible = filtered_catalog(&source, &["artist:blue group".into()]).unwrap();
    assert!(visible.tracks.is_empty());
    assert!(visible.artists.is_empty());
    assert!(visible.labels.is_empty());
    assert!(visible.genres.is_empty());
}
#[test]
fn unknown_blocked_references_are_retained_for_future_scans_without_mutating_graph() {
    let source = catalogue();
    let visible = filtered_catalog(&source, &["artist:future group".into()]).unwrap();
    assert_eq!(visible.tracks.len(), source.tracks.len());
    assert!(filtered_catalog(&source, &["track:/wrong-kind".into()]).is_err());
}
#[test]
fn work_credits_and_path_keyed_measurements_follow_the_same_visibility() {
    let mut source = catalogue();
    let blue = EntityRef::parse_token("artist:blue group")
        .unwrap()
        .resolve(&source)
        .unwrap();
    let gold = EntityRef::parse_token("track:/library/Gold/01.flac")
        .unwrap()
        .resolve(&source)
        .unwrap();
    let recording = source.track(gold).unwrap().recording_id;
    source.works.push(crate::model::Work {
        id: 0,
        title: "A Work".into(),
        key: "a work".into(),
        mbid: "00000000-0000-4000-8000-000000000001".into(),
        recording_ids: vec![recording],
    });
    source.recordings[recording as usize].work_ids.push(0);
    source.credits.push(Credit {
        artist_id: blue,
        entity_kind: EntityKind::Work,
        entity_id: 0,
        role: "lyricist".into(),
        credited_as: None,
        attributes: vec![],
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    });
    assert!(
        filtered_catalog(&source, &["artist:blue group".into()])
            .unwrap()
            .tracks
            .is_empty()
    );
    source.credits.pop();
    for (index, file) in source.files.iter().enumerate() {
        source
            .file_mtime_subseconds
            .insert(file.path.clone(), index as u32);
        source
            .file_added_at
            .insert(file.path.clone(), index as u64 + 1);
        source.analyses.push(crate::analysis::FileAnalysis {
            path: file.path.clone(),
            ..Default::default()
        });
    }
    let visible = filtered_catalog(&source, &["artist:blue group".into()]).unwrap();
    assert_eq!(visible.analyses.len(), 1);
    assert_eq!(visible.file_added_at.len(), 1);
    assert_eq!(visible.file_mtime_subseconds.len(), 1);
    assert_eq!(visible.works.len(), 1);
    assert_eq!(visible.recordings[0].work_ids, vec![0]);
    assert_eq!(visible.works[0].recording_ids, vec![0]);
}
#[test]
fn malformed_dense_identifiers_are_refused_before_indexing() {
    let mut source = catalogue();
    source.tracks[0].id = u32::MAX;
    assert!(filtered_catalog(&source, &["artist:blue group".into()]).is_err());
}
