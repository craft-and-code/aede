//! What the summaries pass does, proved without a network.
//!
//! Declared in `summaries.rs` with `#[path]`, so this is still that module's
//! own child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::Json;
use aede_core::model::EntityKind;
use aede_core::sources::{ArtistFacts, Confidence, Prose, Sources};

/// A transport that answers from canned text, and remembers what was asked.
struct Canned {
    answers: Vec<Result<String, super::super::fetch::Refusal>>,
    asked: Vec<String>,
}

impl Ask for Canned {
    fn get_json(&mut self, url: &str) -> Result<Json, super::super::fetch::Refusal> {
        self.asked.push(url.to_string());
        match self.answers.remove(0) {
            Ok(text) => Ok(aede_core::json::parse(&text).expect("valid fixture")),
            Err(why) => Err(why),
        }
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, super::super::fetch::Refusal> {
        // These passes ask questions and never download anything; a call here
        // is a mistake worth failing on rather than a case worth answering.
        self.asked.push(url.to_string());
        Err(super::super::fetch::Refusal::Failed(
            "this pass downloads nothing".to_string(),
        ))
    }
}

const ENTITY: &str = r#"{"entities":{"Q11649":{"sitelinks":{
    "enwiki":{"site":"enwiki","title":"Marilyn Manson (band)"}}}}}"#;
const SUMMARY: &str = r#"{"lang":"en","extract":"An American rock band.",
    "content_urls":{"desktop":{"page":"https://en.wikipedia.org/wiki/Marilyn_Manson_(band)"}}}"#;

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_summaries_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a data folder");
    dir
}

/// A layer holding one MusicBrainz artist, with or without a wikidata link.
fn held(wikidata: Option<&str>) -> Sources {
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "marilyn manson".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("mbid".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            wikidata: wikidata.map(str::to_string),
            ..Default::default()
        }),
    });
    sources
}

fn entity() -> EntityRef {
    EntityRef {
        kind: EntityKind::Artist,
        key: "marilyn manson".to_string(),
    }
}

fn stored(sources: &Sources) -> Option<Prose> {
    match &sources.get(&entity(), wikipedia::SOURCE)?.facts {
        Facts::Artist(a) => a.summary.clone(),
        _ => None,
    }
}

#[test]
fn the_two_requests_go_through_wikidata_and_land_on_the_article() {
    let dir = sandbox("walk");
    let path = sources::sources_path(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    let mut transport = Canned {
        answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
        asked: Vec::new(),
    };

    run(
        &mut transport,
        &[],
        &["en".to_string()],
        &mut layer,
        &path,
        false,
    )
    .expect("the pass ran");

    assert_eq!(
        transport.asked,
        vec![
            "https://www.wikidata.org/wiki/Special:EntityData/Q11649.json",
            "https://en.wikipedia.org/api/rest_v1/page/summary/Marilyn_Manson_%28band%29",
        ],
        "the entity is the door, and the title behind it is the address"
    );
    let prose = stored(&layer).expect("a summary");
    assert_eq!(prose.text, "An American rock band.");
    assert_eq!(prose.licence, wikipedia::LICENCE);
    assert!(
        !prose.url.is_empty(),
        "the words never arrive without the page they came from"
    );
}

#[test]
fn what_was_stored_is_on_disk_before_the_pass_ends() {
    // Twenty minutes of waiting must not be undone by one interruption: the
    // same rule `fetch` follows, and the reason this asserts on the file
    // rather than on the value in memory.
    let dir = sandbox("saving");
    let path = sources::sources_path(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    let mut transport = Canned {
        answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".to_string()],
        &mut layer,
        &path,
        false,
    )
    .expect("the pass ran");

    let back = sources::load(&path).expect("readable").expect("a layer");
    assert!(
        stored(&back).is_some(),
        "the answer is on disk, not only in memory"
    );
}

#[test]
fn an_artist_with_no_wikidata_link_is_not_asked_about() {
    let dir = sandbox("no_link");
    let path = sources::sources_path(&dir);
    let mut layer = held(None);
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".to_string()],
        &mut layer,
        &path,
        false,
    )
    .expect("the pass ran");
    assert!(
        transport.asked.is_empty(),
        "nothing to look up means nothing is looked up"
    );
}

#[test]
fn an_entity_with_no_article_is_recorded_as_asked_and_empty() {
    // Otherwise every run asks about the same artists forever, which is the
    // one thing an attributed layer is meant to prevent.
    let dir = sandbox("empty");
    let path = sources::sources_path(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    let mut transport = Canned {
        answers: vec![Ok(r#"{"entities":{"Q11649":{"sitelinks":{}}}}"#.to_string())],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".to_string()],
        &mut layer,
        &path,
        false,
    )
    .expect("the pass ran");

    assert_eq!(transport.asked.len(), 1, "the second request is not made");
    let record = layer
        .get(&entity(), wikipedia::SOURCE)
        .expect("a record exists");
    assert!(
        record.facts.is_empty(),
        "asked, and there is no article — which is not the same as never asked"
    );
    assert_eq!(stored(&layer), None);
}

#[test]
fn a_second_run_costs_nothing_unless_it_is_asked_to_do_it_again() {
    let dir = sandbox("again");
    let path = sources::sources_path(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    let mut transport = Canned {
        answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
        asked: Vec::new(),
    };
    let langs = ["en".to_string()];
    run(&mut transport, &[], &langs, &mut layer, &path, false).expect("the first pass");

    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&mut second, &[], &langs, &mut layer, &path, false).expect("the second pass");
    assert!(
        second.asked.is_empty(),
        "already answered, so not asked again"
    );

    let mut third = Canned {
        answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
        asked: Vec::new(),
    };
    run(&mut third, &[], &langs, &mut layer, &path, true).expect("the third pass");
    assert_eq!(third.asked.len(), 2, "--full asks again");
}

#[test]
fn a_failure_on_one_artist_does_not_end_the_pass() {
    let dir = sandbox("failure");
    let path = sources::sources_path(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    let mut transport = Canned {
        answers: vec![Err(super::super::fetch::Refusal::Failed(
            "nope".to_string(),
        ))],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".to_string()],
        &mut layer,
        &path,
        false,
    )
    .expect("a failed lookup is reported, not fatal");
    assert_eq!(
        layer.get(&entity(), wikipedia::SOURCE),
        None,
        "and nothing was recorded, so the next run tries again"
    );
}

#[test]
fn what_fetch_offers_is_exactly_what_this_pass_would_ask() {
    // `fetch` prints this count to offer the second pass. An offer that
    // counted differently from the run it offers is worse than no offer:
    // it is the same function, counted.
    let dir = sandbox("waiting");
    let path = sources::sources_path(&dir);

    assert_eq!(waiting(&held(None)), 0, "no link, nothing to offer");
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    assert_eq!(waiting(&layer), 1);

    let mut transport = Canned {
        answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".to_string()],
        &mut layer,
        &path,
        false,
    )
    .expect("the pass ran");
    assert_eq!(
        waiting(&layer),
        0,
        "once it has been read, the offer stops being made"
    );
}

#[test]
fn the_readers_language_leads_and_english_backs_it_up() {
    assert_eq!(preferred_langs(Some("fr_FR.UTF-8")), ["fr", "en"]);
    assert_eq!(preferred_langs(Some("fr")), ["fr", "en"]);
    assert_eq!(
        preferred_langs(Some("en_GB.UTF-8")),
        ["en"],
        "English is not listed twice"
    );
    // A locale this cannot read must not become a hostname: `https://c.wikipedia.org`
    // does not exist, and the request would fail for a reason nobody could guess.
    for unusable in ["C", "POSIX", "", "en-US-x-lvariant"] {
        assert_eq!(
            preferred_langs(Some(unusable)),
            ["en"],
            "unusable: {unusable}"
        );
    }
    assert_eq!(preferred_langs(None), ["en"]);
}
