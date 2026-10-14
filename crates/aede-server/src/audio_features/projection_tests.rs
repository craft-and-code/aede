use super::*;
use aede_core::user::EntityRef;
use aede_moods::{Features, Statistics};
fn feature() -> Features {
    Features {
        profile: aede_moods::PROFILE,
        frames: 44100,
        windows: 64,
        centroid_hz: Statistics {
            mean: 440.,
            deviation: 0.,
        },
        rolloff_hz: Statistics {
            mean: 500.,
            deviation: 0.,
        },
        flatness: Statistics {
            mean: 0.01,
            deviation: 0.,
        },
        zero_crossing: 0.04,
        chroma: [0.; 12],
        tempo_bpm: None,
        periodicity: None,
        rms_dbfs: Some(-20.),
    }
}
#[test]
fn acoustic_examples_are_owner_private_and_changed_evidence_invalidates_paging() {
    let state = crate::test_support::sample_state();
    let mut guard = state.catalog.try_write().unwrap();
    let catalog = guard.as_mut().unwrap();
    for n in 1..=2 {
        let mut file = catalog.files[0].clone();
        file.id = n;
        file.path = format!("/music/album/{n}.flac");
        catalog.files.push(file);
        let mut track = catalog.tracks[0].clone();
        track.id = n;
        track.file_id = n;
        catalog.tracks.push(track);
        catalog.releases[0].track_ids.push(n);
    }
    let refs = catalog
        .files
        .iter()
        .map(|f| EntityRef::new(EntityKind::Track, f.path.clone()))
        .collect::<Vec<_>>();
    let rows = refs
        .iter()
        .enumerate()
        .map(|(index, reference)| Evidence {
            reference: reference.clone(),
            tags: if index < 2 {
                vec!["calm".into()]
            } else {
                vec![]
            },
            genres: vec![],
            scope: if index < 2 {
                TagScope::Track
            } else {
                TagScope::Genre
            },
        })
        .collect::<Vec<_>>();
    {
        let mut facts = state.audio_features.facts.lock().unwrap();
        for file in &catalog.files {
            facts.rows.insert(
                file.path.clone(),
                Record {
                    input: InputIdentity {
                        path: file.path.clone(),
                        size: file.size,
                        modified: file.mtime,
                        nanos: 0,
                    },
                    features: feature(),
                },
            );
        }
    }
    let rule = RuleView::from(aede_moods::presets().remove(0));
    let one = projection(
        &state,
        catalog,
        &rows,
        std::slice::from_ref(&rule),
        Window {
            chosen: Some("calm"),
            offset: 0,
            limit: 50,
            expected: None,
        },
    )
    .unwrap();
    assert_eq!(one["total"], 3);
    assert_eq!(one["items"][2]["reason"]["scope"], "acoustic");
    let token = one["selection_revision"].as_str().unwrap();
    let other = rows
        .iter()
        .map(|row| Evidence {
            reference: row.reference.clone(),
            tags: vec![],
            genres: vec![],
            scope: TagScope::Genre,
        })
        .collect::<Vec<_>>();
    assert!(
        projection(
            &state,
            catalog,
            &other,
            std::slice::from_ref(&rule),
            Window {
                chosen: Some("calm"),
                offset: 0,
                limit: 50,
                expected: Some(token)
            }
        )
        .is_err()
    );
    assert_eq!(
        projection(
            &state,
            catalog,
            &other,
            &[rule],
            Window {
                chosen: Some("calm"),
                offset: 0,
                limit: 50,
                expected: None
            }
        )
        .unwrap()["total"],
        0
    );
    state.audio_features.facts.lock().unwrap().revision += 1;
    assert!(
        projection(
            &state,
            catalog,
            &rows,
            &[RuleView::from(aede_moods::presets().remove(0))],
            Window {
                chosen: Some("calm"),
                offset: 0,
                limit: 50,
                expected: Some(token)
            }
        )
        .is_err()
    );
}
