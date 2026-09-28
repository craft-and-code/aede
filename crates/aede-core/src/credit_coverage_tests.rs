//! Behavioural tests for credit coverage, separate from production logic.

use super::*;
use crate::model::{AudioFile, Recording, Track};
use crate::sources::{Confidence, CreditLink, SourceRecord, TrackFacts, WorkLink};

fn catalog() -> Catalog {
    Catalog {
        files: vec![
            AudioFile {
                id: 0,
                path: "/music/first.flac".into(),
                ..Default::default()
            },
            AudioFile {
                id: 1,
                path: "/music/reissue.flac".into(),
                ..Default::default()
            },
            AudioFile {
                id: 2,
                path: "/music/unknown.flac".into(),
                ..Default::default()
            },
        ],
        tracks: vec![
            Track {
                id: 0,
                file_id: 0,
                recording_id: 0,
                mbid: Some("recording-1".into()),
                ..Default::default()
            },
            Track {
                id: 1,
                file_id: 1,
                recording_id: 0,
                mbid: Some("recording-1".into()),
                ..Default::default()
            },
            Track {
                id: 2,
                file_id: 2,
                recording_id: 1,
                ..Default::default()
            },
        ],
        recordings: vec![
            Recording {
                id: 0,
                mbid: Some("recording-1".into()),
                track_ids: vec![0, 1],
                ..Default::default()
            },
            Recording {
                id: 1,
                track_ids: vec![2],
                ..Default::default()
            },
        ],
        ..Default::default()
    }
}

fn credit() -> CreditLink {
    CreditLink {
        relation_id: None,
        role_id: None,
        role: "composer".into(),
        direction: None,
        artist_mbid: "artist-1".into(),
        artist_name: "Writer".into(),
        credited_as: None,
        attributes: vec![],
        began: None,
        ended: None,
        over: None,
        order: None,
    }
}

fn source(path: &str, facts: TrackFacts) -> SourceRecord {
    SourceRecord {
        key: path.into(),
        source: MUSICBRAINZ.into(),
        source_id: Some("recording-1".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Track(facts),
    }
}

#[test]
fn distinguishes_unidentified_waiting_incomplete_and_queried_empty() {
    let catalog = catalog();
    let mut held = Sources::default();
    assert_eq!(recordings(&catalog, &held)[0].status, CreditStatus::Waiting);
    assert_eq!(
        recordings(&catalog, &held)[1].status,
        CreditStatus::Unidentified
    );

    held.set(source("/music/first.flac", TrackFacts::default()));
    assert_eq!(recordings(&catalog, &held)[0].status, CreditStatus::Waiting);

    held.set(source(
        "/music/first.flac",
        TrackFacts {
            relationships_complete: true,
            ..Default::default()
        },
    ));
    assert_eq!(recordings(&catalog, &held)[0].status, CreditStatus::Empty);
}

#[test]
fn counts_direct_and_work_credits_once_across_reissues() {
    let catalog = catalog();
    let mut held = Sources::default();
    let facts = TrackFacts {
        credits: vec![credit()],
        works: vec![WorkLink {
            mbid: "work-1".into(),
            credits: vec![credit()],
            ..Default::default()
        }],
        relationships_complete: true,
        ..Default::default()
    };
    held.set(source("/music/first.flac", facts.clone()));
    held.set(source("/music/reissue.flac", facts));
    let rows = recordings(&catalog, &held);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, CreditStatus::Credited);
    assert_eq!(rows[0].recording_credits, 1);
    assert_eq!(rows[0].work_credits, 1);
}

#[test]
fn rejects_conflicted_evidence_but_accepts_trusted_other_placement() {
    let catalog = catalog();
    let mut held = Sources::default();
    let mut conflicting = source(
        "/music/first.flac",
        TrackFacts {
            credits: vec![credit()],
            relationships_complete: true,
            ..Default::default()
        },
    );
    conflicting.source_id = Some("other-recording".into());
    held.set(conflicting);
    assert_eq!(
        recordings(&catalog, &held)[0].status,
        CreditStatus::Untrusted
    );
    held.set(source(
        "/music/reissue.flac",
        TrackFacts {
            relationships_complete: true,
            ..Default::default()
        },
    ));
    assert_eq!(recordings(&catalog, &held)[0].status, CreditStatus::Empty);
}
