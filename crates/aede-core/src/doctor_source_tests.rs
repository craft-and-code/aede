//! Tests for [`super`], split out of `doctor.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::EntityKind;
use crate::sources::{Confidence, Facts, ReleaseFacts, SourceRecord, Sources};
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
