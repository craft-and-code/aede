//! Tests for the album aggregation in `aede credits`.

use super::*;
use aede_core::model::{Recording, Track};

#[test]
fn an_album_counts_a_repeated_recording_only_once() {
    let catalog = Catalog {
        tracks: vec![
            Track {
                id: 0,
                recording_id: 0,
                ..Default::default()
            },
            Track {
                id: 1,
                recording_id: 0,
                ..Default::default()
            },
        ],
        recordings: vec![Recording {
            id: 0,
            ..Default::default()
        }],
        ..Default::default()
    };
    let release = Release {
        track_ids: vec![0, 1],
        ..Default::default()
    };
    let coverage = [RecordingCreditCoverage {
        recording_id: 0,
        status: CreditStatus::Waiting,
        recording_credits: 0,
        work_credits: 0,
    }];
    let rows = album_recordings(&catalog, &release, &coverage);
    assert_eq!(rows.len(), 1);
    assert_eq!(counts(rows.into_iter().map(|row| row.status)).waiting, 1);
}

#[test]
fn edition_json_adds_its_scope_without_changing_recording_coverage_or_pagination() {
    let catalog = Catalog {
        recordings: vec![Recording {
            id: 0,
            title: "Shared recording".into(),
            ..Default::default()
        }],
        tracks: vec![Track {
            id: 0,
            recording_id: 0,
            ..Default::default()
        }],
        releases: vec![
            Release {
                id: 0,
                title: "Original".into(),
                mbid: Some("edition-1".into()),
                track_ids: vec![0],
                ..Default::default()
            },
            Release {
                id: 1,
                title: "Reissue".into(),
                track_ids: vec![0],
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let coverage = [RecordingCreditCoverage {
        recording_id: 0,
        status: CreditStatus::Credited,
        recording_credits: 1,
        work_credits: 2,
    }];
    let editions = [
        EditionCreditCoverage {
            release_id: 0,
            status: CreditStatus::Empty,
            edition_credits: 0,
            manual_credits: 1,
        },
        EditionCreditCoverage {
            release_id: 1,
            status: CreditStatus::Unidentified,
            edition_credits: 0,
            manual_credits: 1,
        },
    ];
    let summary = summary_json(
        &catalog,
        &coverage,
        &editions,
        Window {
            offset: 1,
            limit: 1,
        },
    );
    assert_eq!(
        summary.get("recordings").unwrap().field_u64("total"),
        Some(1)
    );
    assert_eq!(
        summary.get("recordings").unwrap().field_u64("credited"),
        Some(1)
    );
    let summary_editions = summary.get("editions").expect("edition totals");
    for (key, expected) in [
        ("total", 2),
        ("identified", 1),
        ("queried", 1),
        ("credited", 0),
        ("empty", 1),
        ("waiting", 0),
        ("untrusted", 0),
        ("unidentified", 1),
        ("with_manual_credits", 2),
    ] {
        assert_eq!(summary_editions.field_u64(key), Some(expected), "{key}");
    }
    let albums = summary.get("albums").unwrap().as_arr().unwrap();
    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].field_str("album").as_deref(), Some("Reissue"));
    assert_eq!(
        albums[0].get("coverage").unwrap().field_u64("credited"),
        Some(1)
    );
    assert_eq!(
        albums[0]
            .get("edition")
            .unwrap()
            .field_str("status")
            .as_deref(),
        Some("unidentified")
    );

    let album = album_json(
        &catalog,
        &catalog.releases[0],
        &coverage,
        &editions,
        Some(Window {
            offset: 0,
            limit: 1,
        }),
    );
    let edition = album.get("edition").unwrap();
    assert_eq!(edition.field_str("status").as_deref(), Some("empty"));
    assert_eq!(edition.field_u64("edition_credits"), Some(0));
    assert_eq!(edition.field_u64("manual_credits"), Some(1));
    let recordings = album.get("recordings").unwrap().as_arr().unwrap();
    assert_eq!(recordings.len(), 1);
    assert_eq!(recordings[0].field_u64("recording_credits"), Some(1));
    assert_eq!(recordings[0].field_u64("work_credits"), Some(2));
}
