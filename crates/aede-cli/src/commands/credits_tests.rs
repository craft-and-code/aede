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
