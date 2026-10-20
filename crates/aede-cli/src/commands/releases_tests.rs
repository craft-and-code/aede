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
            fingerprint: None,
        }],
        vec!["/music".to_string()],
        1,
        &[],
    )
}

fn only(catalog: &Catalog) -> Target {
    let mut found = targets(
        catalog,
        &Sources::default(),
        &[],
        &crate::commands::fetch::EVERYTHING,
        false,
    );
    assert_eq!(found.len(), 1, "one album in this library");
    found.remove(0)
}

struct Canned {
    answers: Vec<aede_core::json::Json>,
    asked: Vec<String>,
}

impl Ask for Canned {
    fn get_json(&mut self, url: &str) -> Result<aede_core::json::Json, Refusal> {
        self.asked.push(url.to_string());
        Ok(self.answers.remove(0))
    }

    fn get_bytes(&mut self, _: &str) -> Result<Vec<u8>, Refusal> {
        unreachable!("release identification never downloads images")
    }
}

#[test]
fn redirected_release_group_lookups_preserve_identity_and_review_after_reload() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let unrelated = "33333333-3333-4333-8333-333333333333";
    let catalog = library(
        "Example Album",
        "Example Band",
        &[("musicbrainz_releasegroupid", queried)],
    );
    let entity = only(&catalog).entity;
    for previous in [queried, canonical, unrelated] {
        for decision in [
            None,
            Some(sources::ReviewDecision::Accepted),
            Some(sources::ReviewDecision::Rejected),
        ] {
            let mut held = Sources::default();
            held.set(SourceRecord {
                key: entity.key.clone(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(previous.into()),
                fetched_at: 1,
                confidence: Confidence::matched(95),
                facts: Facts::Release(ReleaseFacts::default()),
            });
            if let Some(decision) = decision {
                held.set_review(sources::SourceReview {
                    entity: entity.clone(),
                    source: sources::MUSICBRAINZ.into(),
                    source_id: Some(previous.into()),
                    decision,
                    reviewed_at: 17,
                });
            }
            let path = std::env::temp_dir().join(format!(
                "aede_group_redirect_{}_{previous}_{decision:?}.json",
                std::process::id()
            ));
            let expected = if previous == queried || previous == canonical {
                decision
            } else {
                None
            };
            for returned in [canonical, unrelated] {
                let mut transport = Canned {
                    answers: vec![parse(&format!(r#"{{"id":"{returned}","title":"Example Album","primary-type":"Album"}}"#)).unwrap()],
                    asked: Vec::new(),
                };
                assert_eq!(
                    run(
                        &mut transport,
                        &[],
                        &[only(&catalog)],
                        &mut held,
                        &path,
                        0,
                        1
                    )
                    .unwrap(),
                    (1, 0, 0)
                );
                held = sources::load_all(&path).unwrap().unwrap();
                let record = held.get(&entity, sources::MUSICBRAINZ).unwrap();
                assert_eq!(
                    held.review_for(record)
                        .map(|review| (review.decision, review.reviewed_at)),
                    expected.map(|decision| (decision, 17)),
                    "only a decision about either confirmed identifier follows the merge"
                );
                assert_eq!(
                    held.is_trusted(&catalog, record),
                    expected != Some(sources::ReviewDecision::Rejected),
                    "a merged group lookup must not conflict with the local queried alias"
                );
                let retagged = library(
                    "Example Album",
                    "Example Band",
                    &[("musicbrainz_releasegroupid", returned)],
                );
                let reloaded = sources::load_all(&path).unwrap().unwrap();
                let retagged_record = reloaded.get(&entity, sources::MUSICBRAINZ).unwrap();
                assert_eq!(
                    reloaded.is_trusted(&retagged, retagged_record),
                    expected != Some(sources::ReviewDecision::Rejected),
                    "retagging with the confirmed canonical group must preserve trust"
                );
                assert_eq!(record.source_id.as_deref(), Some(returned));
                assert_eq!(record.musicbrainz_lookup_alias(), Some(queried));
                assert_eq!(
                    record.musicbrainz_release_identity(),
                    Some(sources::MusicBrainzReleaseIdentity::ReleaseGroup(returned))
                );
                assert_eq!(transport.asked.len(), 1);
                assert!(transport.asked[0].contains(&format!("/release-group/{queried}?")));
            }
            std::fs::remove_file(path).unwrap();
        }
    }
}

#[test]
fn malformed_release_group_ids_preserve_the_last_claim_and_remain_retryable() {
    let group = "11111111-1111-4111-8111-111111111111";
    let edition = "44444444-4444-4444-8444-444444444444";
    for route in ["group", "edition", "search"] {
        let ids = match route {
            "group" => vec![("musicbrainz_releasegroupid", group)],
            "edition" => vec![("musicbrainz_albumid", edition)],
            _ => vec![],
        };
        let catalog = library("Example Album", "Example Band", &ids);
        let entity = only(&catalog).entity;
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: entity.key.clone(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(group.into()),
            fetched_at: 1,
            confidence: Confidence::matched(95),
            facts: Facts::Release(ReleaseFacts::default()),
        });
        held.set_review(sources::SourceReview {
            entity: entity.clone(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(group.into()),
            decision: sources::ReviewDecision::Accepted,
            reviewed_at: 17,
        });
        let path = std::env::temp_dir().join(format!(
            "aede_group_admission_{}_{route}.json",
            std::process::id()
        ));
        sources::save(&held, &path).unwrap();
        let before = std::fs::read(&path).unwrap();
        for id in ["not-an-mbid", "", "11111111-1111-4111-8111-11111111111Z"] {
            let row = format!(r#"{{"id":"{id}","title":"Example Album","score":100}}"#);
            let answer = match route {
                "edition" => {
                    format!(r#"{{"id":"{edition}","relations":[],"release-group":{row}}}"#)
                }
                "search" => format!(r#"{{"release-groups":[{row}]}}"#),
                _ => row,
            };
            let mut transport = Canned {
                answers: vec![parse(&answer).unwrap()],
                asked: Vec::new(),
            };
            let selected = targets(&catalog, &held, &[], &super::super::fetch::EVERYTHING, true);
            assert_eq!(
                run(&mut transport, &[], &selected, &mut held, &path, 0, 1).unwrap(),
                (0, 1, 0),
                "{route}: {id}"
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "unreadable answers cannot erase facts or a review"
            );
        }
        let valid = format!(r#"{{"id":"{group}","title":"Example Album","score":95}}"#);
        let answer = match route {
            "edition" => format!(r#"{{"id":"{edition}","relations":[],"release-group":{valid}}}"#),
            "search" => format!(r#"{{"release-groups":[{valid}]}}"#),
            _ => valid,
        };
        let selected = targets(&catalog, &held, &[], &super::super::fetch::EVERYTHING, true);
        let mut transport = Canned {
            answers: vec![parse(&answer).unwrap()],
            asked: Vec::new(),
        };
        assert_eq!(
            run(&mut transport, &[], &selected, &mut held, &path, 0, 1).unwrap(),
            (1, 0, 0)
        );
        assert_eq!(
            sources::load_all(&path)
                .unwrap()
                .unwrap()
                .get(&entity, sources::MUSICBRAINZ)
                .unwrap()
                .source_id
                .as_deref(),
            Some(group)
        );
        std::fs::remove_file(path).unwrap();
    }
    let search = library("Example Album", "Example Band", &[]);
    let answer = parse(&format!(r#"{{"release-groups":[{{"id":"not-an-mbid","title":"Example Album","score":100}},{{"id":"{group}","title":"Example Album","score":95}}]}}"#)).unwrap();
    let (candidate, confidence) = only(&search).read(&answer).unwrap();
    assert_eq!(
        candidate.mbid, group,
        "a malformed leading result must not hide a usable candidate"
    );
    assert_eq!(confidence, Confidence::matched(95));
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
fn a_redirected_edition_keeps_canonical_and_queried_ids_in_the_edition_namespace() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let group = "33333333-3333-4333-8333-333333333333";
    for grouped in [false, true] {
        let catalog = library(
            "Example Album",
            "Example Band",
            &[("musicbrainz_albumid", queried)],
        );
        let nested = if grouped {
            format!(r#", "release-group":{{"id":"{group}"}}"#)
        } else {
            String::new()
        };
        let answer = parse(&format!(r#"{{"id":"{canonical}","relations":[]{nested}}}"#)).unwrap();
        let (candidate, confidence) = only(&catalog).read(&answer).unwrap();
        assert!(confidence.is_certain());
        assert_eq!(
            candidate.facts.edition_mbid.as_deref(),
            Some(canonical),
            "edition facts retain the canonical ID returned by the lookup"
        );
        assert_eq!(
            candidate.mbid,
            if grouped { group } else { canonical },
            "an edition fallback must not be reclassified as a release group"
        );
        assert_eq!(
            candidate.facts.edition_lookup_mbid.as_deref(),
            Some(queried)
        );
    }
}

#[test]
fn unreadable_edition_redirects_cannot_become_completed_release_group_answers() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let group = "33333333-3333-4333-8333-333333333333";
    let catalog = library(
        "Example Album",
        "Example Band",
        &[("musicbrainz_albumid", queried)],
    );
    for id in [
        "null",
        "false",
        "42",
        "[]",
        "{}",
        r#""""#,
        r#""not-an-mbid""#,
    ] {
        let answer = parse(&format!(
            r#"{{"id":{id},"relations":[],"release-group":{{"id":"{group}"}}}}"#
        ))
        .unwrap();
        assert!(
            only(&catalog).read(&answer).is_err(),
            "unreadable edition: {id}"
        );
    }
    let absent = parse(&format!(
        r#"{{"relations":[],"release-group":{{"id":"{group}"}}}}"#
    ))
    .unwrap();
    assert!(
        only(&catalog).read(&absent).is_err(),
        "a group alone does not confirm an edition lookup"
    );
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
            "release-group":{"id":"55555555-5555-4555-8555-555555555555","title":"Kind of Blue",
              "primary-type":"Album","first-release-date":"1959-08-17"}}"#,
    )
    .expect("a fixture");
    let (candidate, confidence) = only(&identified).read(&answer).expect("an answer");
    assert_eq!(confidence, Confidence::Identified);
    assert_eq!(
        candidate.facts,
        ReleaseFacts {
            lookup_mbid: None,
            primary_type: Some("Album".to_string()),
            secondary_types: vec![],
            first_released: Some("1959-08-17".to_string()),
            label: Some("Columbia".to_string()),
            label_mbid: None,
            cover_art: None,
            edition_mbid: Some("59211ea4".to_string()),
            edition_lookup_mbid: None,
            credits: vec![],
            relationships_complete: true,
        }
    );

    let searched = library("Kind of Blue", "Miles Davis", &[]);
    let answer = parse(
        r#"{"release-groups":[{"id":"55555555-5555-4555-8555-555555555555","score":88,"title":"Kind of Blue",
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
             {"id":"66666666-6666-4666-8666-666666666666","score":52,"title":"Greatest Hits Vol. 2"},
             {"id":"77777777-7777-4777-8777-777777777777","score":50,"title":"The Greatest"}]}"#,
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
    let entity = targets(
        &catalog,
        &held,
        &[],
        &crate::commands::fetch::EVERYTHING,
        false,
    )[0]
    .entity
    .clone();
    assert_eq!(
        targets(
            &catalog,
            &held,
            &[],
            &crate::commands::fetch::EVERYTHING,
            false
        )
        .len(),
        1
    );

    held.set(SourceRecord {
        key: entity.key,
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("c9fdb94c".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts::default()),
    });
    assert!(
        targets(
            &catalog,
            &held,
            &[],
            &crate::commands::fetch::EVERYTHING,
            false
        )
        .is_empty(),
        "a second run costs what changed"
    );
    assert_eq!(
        targets(
            &catalog,
            &held,
            &[],
            &crate::commands::fetch::EVERYTHING,
            true
        )
        .len(),
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
        targets(
            &catalog,
            &held,
            &["manson".to_string()],
            &crate::commands::fetch::EVERYTHING,
            false
        )
        .len(),
        1,
        "matched on the album artist"
    );
    assert_eq!(
        targets(
            &catalog,
            &held,
            &["antichrist".to_string()],
            &crate::commands::fetch::EVERYTHING,
            false
        )
        .len(),
        1,
        "and on the title"
    );
    assert!(
        targets(
            &catalog,
            &held,
            &["coltrane".to_string()],
            &crate::commands::fetch::EVERYTHING,
            false
        )
        .is_empty(),
        "and on neither, when neither matches"
    );
}
