//! Behavioural tests for credit coverage, separate from production logic.

use super::*;
use crate::model::{AudioFile, Recording, Release, ReleaseGroup, Track};
use crate::sources::{
    Confidence, CreditLink, ReleaseFacts, ReviewDecision, SourceRecord, SourceReview, TrackFacts,
    WorkLink,
};

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

fn edition_catalog() -> Catalog {
    let mut catalog = catalog();
    catalog.release_groups.push(ReleaseGroup {
        id: 0,
        mbid: "group-1".into(),
        ..Default::default()
    });
    catalog.releases = [Some("edition-1"), Some("edition-2"), None, Some(" ")]
        .into_iter()
        .enumerate()
        .map(|(id, mbid)| Release {
            id: id as Id,
            title: format!("Edition {id}"),
            folder: format!("/music/edition-{id}"),
            mbid: mbid.map(str::to_string),
            release_group_id: Some(0),
            track_ids: if id < 3 { vec![id as Id] } else { vec![] },
            ..Default::default()
        })
        .collect();
    catalog
}

fn edition_source(catalog: &Catalog, release_id: Id, facts: ReleaseFacts) -> SourceRecord {
    SourceRecord {
        key: EntityRef::of(catalog, EntityKind::Release, release_id)
            .unwrap()
            .key,
        source: MUSICBRAINZ.into(),
        source_id: Some("group-1".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(facts),
    }
}

#[test]
fn unusable_recording_identifiers_are_unidentified_even_with_completed_evidence() {
    let mut catalog = catalog();
    let mut held = Sources::default();
    held.set(source(
        "/music/first.flac",
        TrackFacts {
            relationships_complete: true,
            credits: vec![credit()],
            ..Default::default()
        },
    ));
    for mbid in [None, Some(""), Some(" \t ")] {
        catalog.recordings[0].mbid = mbid.map(str::to_string);
        let row = recordings(&catalog, &held)[0];
        assert_eq!(row.status, CreditStatus::Unidentified, "{mbid:?}");
        assert_eq!(row.recording_credits, 0);
        assert_eq!(row.work_credits, 0);
    }
}

#[test]
fn edition_coverage_requires_a_completed_exact_lookup_and_a_usable_local_id() {
    let catalog = edition_catalog();
    let mut held = Sources::default();
    let rows = editions(&catalog, &held);
    assert_eq!(rows[0].status, CreditStatus::Waiting);
    assert_eq!(rows[2].status, CreditStatus::Unidentified);
    assert_eq!(rows[3].status, CreditStatus::Unidentified);
    for (id, edition_mbid) in [(2, None), (3, Some(" "))] {
        held.set(edition_source(
            &catalog,
            id,
            ReleaseFacts {
                edition_mbid: edition_mbid.map(str::to_string),
                relationships_complete: true,
                credits: vec![credit()],
                ..Default::default()
            },
        ));
        let row = editions(&catalog, &held)[id as usize];
        assert_eq!(row.status, CreditStatus::Unidentified);
        assert_eq!(row.edition_credits, 0);
    }
    for edition_id in [None, Some(""), Some(" "), Some("other-edition")] {
        held.set(edition_source(
            &catalog,
            0,
            ReleaseFacts {
                edition_mbid: edition_id.map(str::to_string),
                relationships_complete: true,
                credits: vec![credit()],
                ..Default::default()
            },
        ));
        assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Waiting);
    }
    held.set(edition_source(
        &catalog,
        0,
        ReleaseFacts {
            edition_mbid: Some("edition-1".into()),
            ..Default::default()
        },
    ));
    assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Waiting);
    held.set(edition_source(
        &catalog,
        0,
        ReleaseFacts {
            edition_mbid: Some("edition-1".into()),
            relationships_complete: true,
            ..Default::default()
        },
    ));
    let mut unknown = edition_source(
        &catalog,
        0,
        ReleaseFacts {
            edition_mbid: Some("edition-2".into()),
            relationships_complete: true,
            credits: vec![credit()],
            ..Default::default()
        },
    );
    unknown.key = "unknown-local-edition".into();
    held.set(unknown);
    let rows = editions(&catalog, &held);
    assert_eq!(rows[0].status, CreditStatus::Empty);
    assert_eq!(rows[0].edition_credits, 0);
    assert_eq!(rows[1].status, CreditStatus::Waiting);
}

#[test]
fn recording_and_work_credits_do_not_fill_an_edition_or_its_reissue() {
    let catalog = edition_catalog();
    let mut held = Sources::default();
    held.set(source(
        "/music/first.flac",
        TrackFacts {
            relationships_complete: true,
            credits: vec![credit()],
            works: vec![WorkLink {
                mbid: "work-1".into(),
                credits: vec![credit()],
                ..Default::default()
            }],
            ..Default::default()
        },
    ));
    held.set(edition_source(
        &catalog,
        0,
        ReleaseFacts {
            edition_mbid: Some("edition-1".into()),
            relationships_complete: true,
            credits: vec![credit()],
            ..Default::default()
        },
    ));
    let rows = editions(&catalog, &held);
    assert_eq!(rows[0].status, CreditStatus::Credited);
    assert_eq!(rows[0].edition_credits, 1);
    assert_eq!(rows[1].status, CreditStatus::Waiting);
    assert_eq!(rows[1].edition_credits, 0);
    let recording = recordings(&catalog, &held)[0];
    assert_eq!(recording.recording_credits, 1);
    assert_eq!(recording.work_credits, 1);
}

#[test]
fn edition_reviews_remain_bound_to_the_exact_claim_without_widening_its_scope() {
    let catalog = edition_catalog();
    let mut held = Sources::default();
    let mut record = edition_source(
        &catalog,
        0,
        ReleaseFacts {
            edition_mbid: Some("edition-1".into()),
            relationships_complete: true,
            credits: vec![credit()],
            ..Default::default()
        },
    );
    record.confidence = Confidence::matched(80);
    held.set(record.clone());
    assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Untrusted);
    for (decision, status) in [
        (ReviewDecision::Rejected, CreditStatus::Untrusted),
        (ReviewDecision::Accepted, CreditStatus::Credited),
    ] {
        held.set_review(SourceReview {
            entity: record.entity(),
            source: record.source.clone(),
            source_id: record.source_id.clone(),
            decision,
            reviewed_at: 2,
        });
        assert_eq!(editions(&catalog, &held)[0].status, status);
    }
    record.source_id = Some("other-group".into());
    held.set(record.clone());
    assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Untrusted);
    held.set_review(SourceReview {
        entity: record.entity(),
        source: record.source.clone(),
        source_id: record.source_id.clone(),
        decision: ReviewDecision::Accepted,
        reviewed_at: 3,
    });
    assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Credited);
    let Facts::Release(facts) = &mut record.facts else {
        panic!("release");
    };
    facts.edition_mbid = Some("edition-2".into());
    held.set(record);
    assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Waiting);
    assert_eq!(editions(&catalog, &held)[1].status, CreditStatus::Waiting);
}

#[test]
fn manual_edition_corrections_are_visible_without_fabricating_a_completed_lookup() {
    let catalog = edition_catalog();
    let mut held = Sources::default();
    for id in [0, 2] {
        let mut manual = edition_source(
            &catalog,
            id,
            ReleaseFacts {
                credits: vec![credit()],
                ..Default::default()
            },
        );
        manual.source = "manual".into();
        manual.source_id = None;
        held.set(manual);
    }
    let rows = editions(&catalog, &held);
    assert_eq!(rows[0].status, CreditStatus::Waiting);
    assert_eq!(rows[0].manual_credits, 1);
    assert_eq!(rows[2].status, CreditStatus::Unidentified);
    assert_eq!(rows[2].manual_credits, 1);
    assert_eq!(rows[0].edition_credits, 0);
    assert_eq!(rows[1].manual_credits, 0);
    for (decision, expected) in [(ReviewDecision::Rejected, 0), (ReviewDecision::Accepted, 1)] {
        held.set_review(SourceReview {
            entity: EntityRef::of(&catalog, EntityKind::Release, 0).unwrap(),
            source: "manual".into(),
            source_id: None,
            decision,
            reviewed_at: 2,
        });
        assert_eq!(editions(&catalog, &held)[0].manual_credits, expected);
        assert_eq!(editions(&catalog, &held)[0].status, CreditStatus::Waiting);
    }
    let mut scoped = edition_source(
        &catalog,
        0,
        ReleaseFacts {
            edition_mbid: Some("edition-2".into()),
            credits: vec![credit()],
            ..Default::default()
        },
    );
    scoped.source = "manual".into();
    scoped.source_id = None;
    held.set(scoped);
    assert_eq!(editions(&catalog, &held)[0].manual_credits, 0);
}
