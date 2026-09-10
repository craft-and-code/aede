//! What the labels pass decides, proved without a network.
//!
//! Declared in `labels.rs` with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, ReleaseFacts, Sources};
use aede_core::tags::RawTags;

/// A one-album catalog whose release is tagged with the given label.
fn catalog_with_label(label: &str) -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("label", label);
    build(
        vec![ScannedFile {
            path: "/music/Miles Davis/Kind of Blue/01.flac".to_string(),
            size: 1,
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

/// A `Facts::Release` record, as a release lookup would leave it, naming the
/// given label and — when one is given — its identifier.
fn release_record(key: &str, label: &str, label_mbid: Option<&str>) -> SourceRecord {
    SourceRecord {
        key: key.to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("release-mbid".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            label: Some(label.to_string()),
            label_mbid: label_mbid.map(str::to_string),
            ..Default::default()
        }),
    }
}

/// [`targets`] over the whole library, unnarrowed — the shape every test
/// below wants, `again` aside.
fn targets_over(catalog: &Catalog, held: &Sources, again: bool) -> Vec<Target> {
    targets(
        catalog,
        held,
        &[],
        &crate::commands::fetch::EVERYTHING,
        again,
    )
}

#[test]
fn a_label_with_no_release_lookup_yet_is_a_target_with_no_known_identifier() {
    let catalog = catalog_with_label("Columbia");
    let held = Sources::default();
    let list = targets_over(&catalog, &held, false);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Columbia");
    assert!(
        list[0].known_mbid.is_none(),
        "nothing has named this label's identifier yet, so this asks a search"
    );
}

#[test]
fn known_mbid_finds_the_identifier_a_release_lookup_already_named() {
    let catalog = catalog_with_label("Columbia");
    let mut held = Sources::default();
    held.set(release_record(
        "miles davis|kind of blue|music/Miles Davis/Kind of Blue",
        "Columbia",
        Some("columbia-mbid"),
    ));
    let list = targets_over(&catalog, &held, false);
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].known_mbid.as_deref(),
        Some("columbia-mbid"),
        "a release this catalog looked up already named this label's identifier"
    );
}

#[test]
fn known_mbid_ignores_a_release_naming_a_different_label() {
    let catalog = catalog_with_label("Columbia");
    let mut held = Sources::default();
    held.set(release_record(
        "some|other|release",
        "EMI",
        Some("emi-mbid"),
    ));
    let list = targets_over(&catalog, &held, false);
    assert_eq!(list.len(), 1);
    assert!(
        list[0].known_mbid.is_none(),
        "the identifier on record names a different label"
    );
}

#[test]
fn known_mbid_ignores_a_release_named_but_with_no_identifier() {
    // Passive capture only fills `label_mbid` when the very same `label-info`
    // entry carried one — plenty of releases name a label and nothing else.
    let catalog = catalog_with_label("Columbia");
    let mut held = Sources::default();
    held.set(release_record("some|other|release", "Columbia", None));
    let list = targets_over(&catalog, &held, false);
    assert_eq!(list.len(), 1);
    assert!(list[0].known_mbid.is_none());
}

#[test]
fn a_label_already_answered_is_not_a_target_again() {
    let catalog = catalog_with_label("Columbia");
    let mut held = Sources::default();
    let entity = EntityRef::of(&catalog, EntityKind::Label, catalog.labels[0].id)
        .expect("the label is in the catalog");
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("columbia-mbid".to_string()),
        fetched_at: 1,
        confidence: Confidence::matched(90),
        facts: Facts::Label(LabelFacts::default()),
    });

    let list = targets_over(&catalog, &held, false);
    assert!(list.is_empty(), "already asked, and answered");

    let again = targets_over(&catalog, &held, true);
    assert_eq!(
        again.len(),
        1,
        "--full asks again even with an identifier on file"
    );
}

#[test]
fn a_catalog_with_no_labels_has_nothing_to_ask_about() {
    let empty = build(vec![], vec!["/music".to_string()], 1, &[]);
    let held = Sources::default();
    let list = targets_over(&empty, &held, false);
    assert!(list.is_empty());
}

#[test]
fn a_name_that_does_not_reach_the_label_leaves_it_out() {
    let catalog = catalog_with_label("Columbia");
    let held = Sources::default();
    let narrowed = targets(
        &catalog,
        &held,
        &["warner".to_string()],
        &crate::commands::fetch::EVERYTHING,
        false,
    );
    assert!(narrowed.is_empty(), "\"warner\" names a different label");

    let matching = targets(
        &catalog,
        &held,
        &["columbia".to_string()],
        &crate::commands::fetch::EVERYTHING,
        false,
    );
    assert_eq!(matching.len(), 1);
}

#[test]
fn waiting_counts_exactly_what_targets_would_ask_about() {
    let catalog = catalog_with_label("Columbia");
    let held = Sources::default();
    assert_eq!(waiting(&catalog, &held), 1);
}
