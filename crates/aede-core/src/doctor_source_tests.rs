//! Tests for [`super`], split out of `doctor.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::EntityKind;
use crate::sources::{
    Confidence, Facts, LabelFacts, ReleaseFacts, ReviewDecision, SourceRecord, Sources, TrackFacts,
};
use crate::user::EntityRef;

/// A one-album catalog whose tags say what is given.
fn catalog_with(label: &str, date: &str) -> Catalog {
    use crate::model::builder::{ScannedFile, build};
    use crate::tags::RawTags;
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("tracknumber", "1");
    tags.insert("label", label);
    tags.insert("date", date);
    build(
        vec![ScannedFile {
            path: "/music/Miles/Kind of Blue/01.flac".to_string(),
            size: 10,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    )
}

fn said(catalog: &Catalog, facts: ReleaseFacts) -> Sources {
    let entity = EntityRef::of(catalog, EntityKind::Release, 0).expect("a release");
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: entity.key,
        source: "musicbrainz".to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(facts),
    });
    sources
}

#[test]
fn a_source_contradicting_a_tag_is_reported_and_not_resolved() {
    let catalog = catalog_with("Columbia", "1959");
    let sources = said(
        &catalog,
        ReleaseFacts {
            label: Some("Blue Note".to_string()),
            ..Default::default()
        },
    );
    let issues = diagnose(&catalog, &sources);
    let found: Vec<&Issue> = issues
        .iter()
        .filter(|i| i.kind == IssueKind::SourceDisagrees)
        .collect();
    assert_eq!(found.len(), 1, "issues: {issues:?}");

    // Both sides are named, because which of the two is right is not
    // something this program can decide.
    assert!(found[0].detail.contains("Blue Note"), "{}", found[0].detail);
    assert!(found[0].detail.contains("Columbia"), "{}", found[0].detail);
    assert!(
        found[0].detail.contains("musicbrainz"),
        "and who says so: {}",
        found[0].detail
    );
    // Information, not a defect: a tag may be wrong and so may a source.
    assert_eq!(found[0].severity(), Severity::Info);
}

#[test]
fn an_approximate_identity_is_reviewed_before_incomplete_credits_are_trusted() {
    let catalog = catalog_with("Columbia", "1959");
    let entity = EntityRef::of(&catalog, EntityKind::Track, 0).expect("a track");
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: entity.key,
        source: "musicbrainz".to_string(),
        source_id: Some("possible-recording".to_string()),
        fetched_at: 1,
        confidence: Confidence::matched(96),
        facts: Facts::Track(TrackFacts {
            recording: Some("possible-recording".to_string()),
            relationships_complete: false,
            ..Default::default()
        }),
    });

    let issues = diagnose(&catalog, &sources);
    assert!(
        issues
            .iter()
            .any(|issue| issue.kind == IssueKind::SourceNeedsReview)
    );
    assert!(
        !issues
            .iter()
            .any(|issue| issue.kind == IssueKind::IncompleteSourceCredits),
        "untrusted evidence must not claim that a graph relationship is incomplete"
    );

    let id = sources.review_items(&catalog)[0].id.clone();
    sources
        .decide(&catalog, &id, ReviewDecision::Accepted, 2)
        .expect("accepted");
    let issues = diagnose(&catalog, &sources);
    assert!(
        !issues
            .iter()
            .any(|issue| issue.kind == IssueKind::SourceNeedsReview)
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue.kind == IssueKind::IncompleteSourceCredits)
    );
}

#[test]
fn a_completed_recording_on_another_placement_clears_the_incomplete_credit_diagnostic() {
    use crate::model::{ScannedFile, build};
    use crate::sources::{MUSICBRAINZ, SourceReview};
    use crate::tags::RawTags;

    let local = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let catalog = build(
        ["first", "reissue"]
            .into_iter()
            .map(|album| {
                let mut tags = RawTags::default();
                for (key, value) in [
                    ("artist", "Example Artist"),
                    ("albumartist", "Example Artist"),
                    ("album", album),
                    ("title", "Example Recording"),
                    ("musicbrainz_trackid", local),
                ] {
                    tags.insert(key, value);
                }
                ScannedFile {
                    path: format!("/music/{album}/track.flac"),
                    size: 10,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }
            })
            .collect(),
        vec!["/music".into()],
        1,
        &[],
    );
    assert_eq!(catalog.recordings.len(), 1);
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: catalog.files[0].path.clone(),
        source: MUSICBRAINZ.into(),
        source_id: Some(local.into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Track(TrackFacts {
            recording: Some(local.into()),
            ..Default::default()
        }),
    });
    assert!(
        diagnose(&catalog, &sources)
            .iter()
            .any(|issue| { issue.kind == IssueKind::IncompleteSourceCredits })
    );

    for (id, recording, complete, confidence, review, incomplete) in [
        (local, local, true, Confidence::Identified, None, false),
        (
            canonical,
            canonical,
            true,
            Confidence::Identified,
            None,
            false,
        ),
        (canonical, local, true, Confidence::Identified, None, true),
        (local, local, false, Confidence::Identified, None, true),
        (local, local, true, Confidence::matched(95), None, true),
        (
            local,
            local,
            true,
            Confidence::matched(95),
            Some(ReviewDecision::Accepted),
            false,
        ),
        (
            local,
            local,
            true,
            Confidence::Identified,
            Some(ReviewDecision::Rejected),
            true,
        ),
    ] {
        let mut held = sources.clone();
        let record = SourceRecord {
            key: catalog.files[1].path.clone(),
            source: MUSICBRAINZ.into(),
            source_id: Some(id.into()),
            fetched_at: 2,
            confidence,
            facts: Facts::Track(TrackFacts {
                recording: Some(recording.into()),
                relationships_for: Some(local.into()),
                relationships_complete: complete,
                ..Default::default()
            }),
        };
        if let Some(decision) = review {
            held.set_review(SourceReview {
                entity: record.entity(),
                source: MUSICBRAINZ.into(),
                source_id: record.source_id.clone(),
                decision,
                reviewed_at: 3,
            });
        }
        held.set(record);
        let snapshot = crate::sources::to_json(&held);
        let mut held = crate::sources::from_json(&snapshot).expect("readable cache");
        for reverse in [false, true] {
            if reverse {
                held.records.reverse();
            }
            let issues = diagnose(&catalog, &held);
            assert_eq!(
                issues
                    .iter()
                    .any(|issue| issue.kind == IssueKind::IncompleteSourceCredits),
                incomplete,
                "id={id}, recording={recording}, complete={complete}, confidence={confidence:?}, review={review:?}, reverse={reverse}"
            );
        }
    }
}

#[test]
fn an_exact_identity_conflict_points_to_the_review_command() {
    let mut catalog = catalog_with("Columbia", "1959");
    catalog.labels[0].mbid = Some("local-label".to_string());
    let entity = EntityRef::of(&catalog, EntityKind::Label, 0).expect("a label");
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: entity.key,
        source: "musicbrainz".to_string(),
        source_id: Some("other-label".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    let issues = diagnose(&catalog, &sources);
    let issue = issues
        .iter()
        .find(|issue| issue.kind == IssueKind::SourceIdentityConflict)
        .expect("identity conflict");
    assert!(issue.detail.contains("aede review --accept="));
    assert!(issue.detail.contains("local-label"));
    assert!(issue.detail.contains("other-label"));
}

#[test]
fn two_trusted_sources_contradicting_each_other_are_reported() {
    let catalog = catalog_with("Columbia", "1959");
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).expect("a release");
    let mut sources = Sources::default();
    for (source, label) in [("musicbrainz", "Columbia"), ("discogs", "CBS")] {
        sources.set(SourceRecord {
            key: entity.key.clone(),
            source: source.to_string(),
            source_id: None,
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Release(ReleaseFacts {
                label: Some(label.to_string()),
                ..Default::default()
            }),
        });
    }
    let issues = diagnose(&catalog, &sources);
    let issue = issues
        .iter()
        .find(|issue| issue.kind == IssueKind::SourcesDisagree)
        .expect("source-to-source disagreement");
    assert!(issue.detail.contains("Columbia"));
    assert!(issue.detail.contains("CBS"));
}

#[test]
fn a_year_against_a_full_date_is_not_reported() {
    // The false alarm that would otherwise land on nearly every album of a
    // library the first time anything is fetched.
    let catalog = catalog_with("Columbia", "1959");
    let sources = said(
        &catalog,
        ReleaseFacts {
            first_released: Some("1959-08-17".to_string()),
            label: Some("Columbia".to_string()),
            ..Default::default()
        },
    );
    let issues = diagnose(&catalog, &sources);
    assert!(
        !issues.iter().any(|i| i.kind == IssueKind::SourceDisagrees),
        "agreement at two precisions is agreement: {issues:?}"
    );
}

#[test]
fn a_field_the_tags_are_silent_about_is_not_a_disagreement() {
    // The source adds something rather than contradicting anything, and a
    // report that could not tell the two apart would be unreadable.
    let catalog = catalog_with("Columbia", "1959");
    let sources = said(
        &catalog,
        ReleaseFacts {
            primary_type: Some("Album".to_string()),
            ..Default::default()
        },
    );
    let issues = diagnose(&catalog, &sources);
    assert!(
        !issues.iter().any(|i| i.kind == IssueKind::SourceDisagrees),
        "no RELEASETYPE tag to disagree with: {issues:?}"
    );
}

#[test]
fn nothing_is_said_about_a_release_this_catalog_does_not_hold() {
    // A record waiting for its album to be scanned is not a defect, and
    // `doctor` reporting it would make an empty library look broken.
    let catalog = catalog_with("Columbia", "1959");
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "somebody|an album nobody scanned|/elsewhere".to_string(),
        source: "musicbrainz".to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            label: Some("Blue Note".to_string()),
            ..Default::default()
        }),
    });
    let issues = diagnose(&catalog, &sources);
    assert!(
        !issues.iter().any(|i| i.kind == IssueKind::SourceDisagrees),
        "issues: {issues:?}"
    );
}
