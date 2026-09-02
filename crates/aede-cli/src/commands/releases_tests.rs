//! What the album half of `fetch` asks, and what it makes of the answer.
//!
//! Declared in `releases.rs` with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::parse;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, ReleaseFacts, Sources};
use aede_core::tags::RawTags;

/// A one-track library, with whatever identifiers the caller wants on it.
fn library(album: &str, artist: &str, ids: &[(&str, &str)]) -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("artist", artist);
    tags.insert("albumartist", artist);
    tags.insert("album", album);
    tags.insert("title", "A track");
    for &(key, value) in ids {
        tags.insert(key, value);
    }
    build(
        vec![ScannedFile {
            path: format!("/music/{artist}/{album}/01.flac"),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        }],
        vec!["/music".to_string()],
        1,
    )
}

fn only(catalog: &Catalog) -> Target {
    let mut found = targets(catalog, &Sources::default(), &[], false);
    assert_eq!(found.len(), 1, "one album in this library");
    found.remove(0)
}

#[test]
fn an_edition_identifier_asks_for_the_label_and_the_album_at_once() {
    // The whole reason `inc=labels+release-groups` exists in this program: at
    // one request per second, asking the edition and then its group would
    // double a run over a library that Picard has already tagged.
    let catalog = library(
        "Kind of Blue",
        "Miles Davis",
        &[("musicbrainz_albumid", "59211ea4")],
    );
    let url = only(&catalog).url();
    assert!(url.contains("/release/59211ea4"), "{url}");
    assert!(url.contains("inc=labels+release-groups"), "{url}");
    assert!(!url.contains("query="), "a lookup, not a search: {url}");
}

#[test]
fn a_group_identifier_alone_asks_for_the_group() {
    let catalog = library(
        "Kind of Blue",
        "Miles Davis",
        &[("musicbrainz_releasegroupid", "c9fdb94c")],
    );
    let url = only(&catalog).url();
    assert!(url.contains("/release-group/c9fdb94c"), "{url}");
    assert!(!url.contains("query="), "still a lookup: {url}");
}

#[test]
fn without_an_identifier_the_artist_narrows_the_search() {
    // A title alone is not a question: "Greatest Hits" answers for a hundred
    // records. The album artist is what makes the search answerable, and it is
    // in the tags whenever the title is.
    let catalog = library("Kind of Blue", "Miles Davis", &[]);
    let url = only(&catalog).url();
    assert!(url.contains("/release-group/?query="), "{url}");
    assert!(url.contains("releasegroup"), "{url}");
    assert!(
        url.contains("artist"),
        "the artist rides in the same query: {url}"
    );
    assert!(
        url.contains("Kind%20of%20Blue") || url.contains("Kind+of+Blue"),
        "the title went out encoded, not raw: {url}"
    );
}

#[test]
fn a_title_is_a_title_and_not_an_expression() {
    // The same fault that cost a whole-library run on the artist side: the
    // search server parses the query, and an unescaped character makes it
    // unparseable — a failure that arrives as 503 and reads as "too fast".
    let catalog = library("Sunn O)))", "Sunn O)))", &[]);
    let url = only(&catalog).url();
    assert!(
        !url.contains("%29%29%29") || url.contains("%5C%29"),
        "the parentheses were escaped before being encoded: {url}"
    );
}

#[test]
fn a_lookup_is_a_certainty_and_a_search_is_scored() {
    let identified = library(
        "Kind of Blue",
        "Miles Davis",
        &[("musicbrainz_albumid", "59211ea4")],
    );
    let answer = parse(
        r#"{"id":"59211ea4","title":"Kind of Blue",
            "label-info":[{"label":{"name":"Columbia"}}],
            "release-group":{"id":"c9fdb94c","title":"Kind of Blue",
              "primary-type":"Album","first-release-date":"1959-08-17"}}"#,
    )
    .expect("a fixture");
    let (candidate, confidence) = only(&identified).read(&answer).expect("an answer");
    assert_eq!(confidence, Confidence::Identified);
    assert_eq!(
        candidate.facts,
        ReleaseFacts {
            primary_type: Some("Album".to_string()),
            secondary_types: vec![],
            first_released: Some("1959-08-17".to_string()),
            label: Some("Columbia".to_string()),
        }
    );

    let searched = library("Kind of Blue", "Miles Davis", &[]);
    let answer = parse(
        r#"{"release-groups":[{"id":"c9fdb94c","score":88,"title":"Kind of Blue",
            "primary-type":"Album","first-release-date":"1959-08-17"}]}"#,
    )
    .expect("a fixture");
    let (_, confidence) = only(&searched).read(&answer).expect("an answer");
    assert!(
        !confidence.is_certain(),
        "a search never produces a certainty: {confidence:?}"
    );
}

#[test]
fn an_album_nothing_clearly_matches_is_left_alone() {
    let catalog = library("Greatest Hits", "Various", &[]);
    let answer = parse(
        r#"{"release-groups":[
             {"id":"a","score":52,"title":"Greatest Hits Vol. 2"},
             {"id":"b","score":50,"title":"The Greatest"}]}"#,
    )
    .expect("a fixture");
    assert!(
        only(&catalog).read(&answer).is_err(),
        "nothing was close enough, so nothing is filed"
    );
}

#[test]
fn an_album_already_answered_is_not_asked_about_twice() {
    let catalog = library("Kind of Blue", "Miles Davis", &[]);
    let mut held = Sources::default();
    let entity = targets(&catalog, &held, &[], false)[0].entity.clone();
    assert_eq!(targets(&catalog, &held, &[], false).len(), 1);

    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("c9fdb94c".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts::default()),
    });
    assert!(
        targets(&catalog, &held, &[], false).is_empty(),
        "a second run costs what changed"
    );
    assert_eq!(
        targets(&catalog, &held, &[], true).len(),
        1,
        "--full asks again"
    );
}

#[test]
fn a_name_on_the_command_line_reaches_the_records_as_well_as_the_person() {
    // `aede fetch manson` should mean one thing. Narrowing the artists by name
    // and the albums by title only would quietly leave the albums out of a run
    // the user thought they had asked for.
    let catalog = library("Antichrist Superstar", "Marilyn Manson", &[]);
    let held = Sources::default();
    assert_eq!(
        targets(&catalog, &held, &["manson".to_string()], false).len(),
        1,
        "matched on the album artist"
    );
    assert_eq!(
        targets(&catalog, &held, &["antichrist".to_string()], false).len(),
        1,
        "and on the title"
    );
    assert!(
        targets(&catalog, &held, &["coltrane".to_string()], false).is_empty(),
        "and on neither, when neither matches"
    );
}
