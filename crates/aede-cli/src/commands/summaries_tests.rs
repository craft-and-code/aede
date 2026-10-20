//! What the summaries pass does, proved without a network.
//!
//! Declared in `summaries.rs` with `#[path]`, so this is still that module's
//! own child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::Json;
use aede_core::model::EntityKind;
use aede_core::sources::{ArtistFacts, Confidence, LabelFacts, Prose, Sources};

/// The options a pass reads, as `fetch` gathers them.
fn asked(again: bool) -> crate::commands::fetch::Asked<'static> {
    crate::commands::fetch::Asked {
        scope: &crate::commands::fetch::EVERYTHING,
        names: &[],
        again,
        dry_run: false,
        size: crate::commands::covers::DEFAULT_SIZE,
        images: false,
        fanart: Default::default(),
        key: None,
        portrait_key: None,
        langs: vec!["en".to_string()],
        size_requested: false,
    }
}
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

/// The test that owns this folder, for a name no other test can produce.
///
/// **Both halves are needed, and each was learnt the hard way.** Naming a
/// folder by the argument alone works only while no two tests pass the same
/// word — a rule nothing enforces and no grep can check, because a helper
/// called from three tests spells the word once: three of them shared a folder,
/// each deleting it as it started, and the race passed on Linux and failed on
/// macOS. Naming it by the test alone then broke the opposite case within a
/// single test, where two sandboxes are two folders on purpose. So the name is
/// the test **and** the argument: unique across tests however they arrive here,
/// unique within one, and the same on the next run, so a re-run still clears
/// what the last one left.
fn owner() -> String {
    std::thread::current()
        .name()
        .map(|name| name.replace("::", "_"))
        .unwrap_or_else(|| "main".to_string())
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_summaries_{}_{name}", owner()));
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

#[test]
fn an_accepted_name_match_completes_its_missing_artist_links_before_the_biography() {
    let dir = sandbox("accepted_artist_links");
    let path = sources::sources_path(&dir);
    let mut layer = held(None);
    let mbid = "11111111-1111-4111-8111-111111111111";
    layer.records[0].source_id = Some(mbid.into());
    layer.records[0].confidence = Confidence::Matched(95);
    let catalog = aede_core::model::Catalog::default();
    let id = layer.review_items(&catalog)[0].id.clone();
    layer
        .decide(&catalog, &id, sources::ReviewDecision::Accepted, 2)
        .unwrap();
    let mut transport = Canned {
        answers: vec![
            Ok(format!(
                r#"{{"id":"{mbid}","name":"Example Band","relations":[{{"type":"wikidata","url":{{"resource":"https://www.wikidata.org/wiki/Q11649"}}}}]}}"#
            )),
            Ok(ENTITY.into()),
            Ok(SUMMARY.into()),
        ],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &path,
        &asked(false),
        Some(&catalog),
    )
    .unwrap();
    assert_eq!(transport.asked.len(), 3);
    assert!(transport.asked[0].contains(&format!("/artist/{mbid}?")));
    assert!(transport.asked[0].contains("url-rels"));
    assert_eq!(layer.records[0].confidence, Confidence::Matched(95));
    assert!(layer.review_for(&layer.records[0]).is_some());
    assert!(layer.get(&entity(), wikipedia::SOURCE).is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_summary_does_not_turn_a_pending_or_rejected_match_into_an_identity() {
    let dir = sandbox("reviewed_identity");
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    layer.records[0].confidence = Confidence::Matched(80);
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
        None,
    )
    .unwrap();
    assert!(
        transport.asked.is_empty(),
        "unreviewed identity must not determine which biography is fetched"
    );
    let catalog = aede_core::model::Catalog::default();
    let id = layer.review_items(&catalog)[0].id.clone();
    layer
        .decide(&catalog, &id, sources::ReviewDecision::Accepted, 2)
        .unwrap();
    transport
        .answers
        .push(Err(super::super::fetch::Refusal::Failed(
            "temporary refusal".into(),
        )));
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
        None,
    )
    .unwrap();
    assert_eq!(
        transport.asked.len(),
        1,
        "an explicit acceptance permits the linked entity"
    );
    layer
        .decide(&catalog, &id, sources::ReviewDecision::Rejected, 3)
        .unwrap();
    transport.asked.clear();
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(true),
        None,
    )
    .unwrap();
    assert!(transport.asked.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

fn stored(sources: &Sources) -> Option<Prose> {
    match &sources.get(&entity(), wikipedia::SOURCE)?.facts {
        Facts::Artist(a) => a.summary.clone(),
        _ => None,
    }
}

#[test]
fn malformed_provider_documents_preserve_biographies_and_remain_retryable() {
    for (stage, malformed) in [
        ("entity", "{}"),
        ("entity", r#"{"entities":[]}"#),
        ("entity", r#"{"entities":{"Q11649":{"sitelinks":[]}}}"#),
        ("summary", "{}"),
        ("summary", r#"{"type":"standard","extract":[]}"#),
    ] {
        for previous in [false, true] {
            let dir = sandbox(&format!("invalid_{stage}_{previous}_{}", malformed.len()));
            let path = sources::sources_path(&dir);
            let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
            if previous {
                let target =
                    targets(&layer, &[], &crate::commands::fetch::EVERYTHING, true, None).remove(0);
                store(
                    &mut layer,
                    &target,
                    Some(Prose {
                        text: "Retained biography.".into(),
                        url: "https://en.wikipedia.org/wiki/Retained".into(),
                        lang: "en".into(),
                        licence: wikipedia::LICENCE.into(),
                    }),
                );
            }
            sources::save(&layer, &path).expect("initial sources");
            let before = std::fs::read(&path).expect("original bytes");
            let original = layer.clone();
            let answers = if stage == "entity" {
                vec![Ok(malformed.to_string())]
            } else {
                vec![Ok(ENTITY.to_string()), Ok(malformed.to_string())]
            };
            let mut transport = Canned {
                answers,
                asked: Vec::new(),
            };
            run(
                &mut transport,
                &[],
                &["en".into()],
                &mut layer,
                &path,
                &asked(true),
                None,
            )
            .expect("failed responses are counted by the pass");
            assert_eq!(layer, original, "malformed {stage}: {malformed}");
            assert_eq!(std::fs::read(&path).expect("source bytes"), before);

            let mut retry = Canned {
                answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
                asked: Vec::new(),
            };
            run(
                &mut retry,
                &[],
                &["en".into()],
                &mut layer,
                &path,
                &asked(previous),
                None,
            )
            .expect("valid retry");
            assert_eq!(
                retry.asked.len(),
                2,
                "failed first fetch was not cached as an absence"
            );
            assert_eq!(
                stored(&layer).expect("biography").text,
                "An American rock band."
            );
            std::fs::remove_dir_all(dir).expect("clean sandbox");
        }
    }
}

#[test]
fn a_label_summary_is_fetched_and_saved_with_its_credit() {
    let dir = sandbox("label_summary");
    let path = sources::sources_path(&dir);
    let mut layer = Sources::default();
    layer.set(SourceRecord {
        key: "roadracer records".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("label-id".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            wikidata: Some("https://www.wikidata.org/wiki/Q11649".to_string()),
            ..Default::default()
        }),
    });
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
        &asked(false),
        None,
    )
    .expect("the pass ran");
    let back = sources::load(&path).expect("readable").expect("a layer");
    let entity = EntityRef {
        kind: EntityKind::Label,
        key: "roadracer records".to_string(),
    };
    let record = back
        .get(&entity, wikipedia::SOURCE)
        .expect("a label answer");
    let Facts::Label(label) = &record.facts else {
        panic!("label facts");
    };
    let prose = label.summary.as_ref().expect("a summary");
    assert_eq!(prose.text, "An American rock band.");
    assert_eq!(prose.licence, wikipedia::LICENCE);
    assert!(prose.credit().contains("en.wikipedia.org"));
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
        &asked(false),
        None,
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
        &asked(false),
        None,
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
        &asked(false),
        None,
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
        &asked(false),
        None,
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
    run(
        &mut transport,
        &[],
        &langs,
        &mut layer,
        &path,
        &asked(false),
        None,
    )
    .expect("the first pass");

    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(
        &mut second,
        &[],
        &langs,
        &mut layer,
        &path,
        &asked(false),
        None,
    )
    .expect("the second pass");
    assert!(
        second.asked.is_empty(),
        "already answered, so not asked again"
    );

    let mut third = Canned {
        answers: vec![Ok(ENTITY.to_string()), Ok(SUMMARY.to_string())],
        asked: Vec::new(),
    };
    run(
        &mut third,
        &[],
        &langs,
        &mut layer,
        &path,
        &asked(true),
        None,
    )
    .expect("the third pass");
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
        &asked(false),
        None,
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

    assert_eq!(waiting(&held(None), None), 0, "no link, nothing to offer");
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    assert_eq!(waiting(&layer, None), 1);

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
        &asked(false),
        None,
    )
    .expect("the pass ran");
    assert_eq!(
        waiting(&layer, None),
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

#[test]
fn pending_rejected_and_previewed_artist_link_lookups_do_not_contact_services() {
    let dir = sandbox("artist_link_admission");
    for decision in [
        None,
        Some(sources::ReviewDecision::Rejected),
        Some(sources::ReviewDecision::Accepted),
    ] {
        let mut layer = held(None);
        layer.records[0].source_id = Some("11111111-1111-4111-8111-111111111111".into());
        layer.records[0].confidence = Confidence::Matched(95);
        let catalog = aede_core::model::Catalog::default();
        if let Some(decision) = decision {
            let id = layer.review_items(&catalog)[0].id.clone();
            layer.decide(&catalog, &id, decision, 2).unwrap();
        }
        let original = layer.clone();
        let mut request = asked(false);
        request.dry_run = decision == Some(sources::ReviewDecision::Accepted);
        let mut transport = Canned {
            answers: Vec::new(),
            asked: Vec::new(),
        };
        run(
            &mut transport,
            &[],
            &["en".into()],
            &mut layer,
            &sources::sources_path(&dir),
            &request,
            Some(&catalog),
        )
        .unwrap();
        assert!(transport.asked.is_empty());
        assert_eq!(layer, original);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn artist_link_failures_preserve_claims_and_a_confirmed_absence_is_not_retried() {
    let dir = sandbox("artist_link_retry");
    let path = sources::sources_path(&dir);
    let id = "11111111-1111-4111-8111-111111111111";
    let mut layer = held(None);
    layer.records[0].source_id = Some(id.into());
    sources::save(&layer, &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for answer in [
        Ok("{}".into()),
        Ok(r#"{"id":"not-a-musicbrainz-id","name":"Example","relations":[]}"#.into()),
        Err(super::super::fetch::Refusal::Failed("unavailable".into())),
    ] {
        let mut transport = Canned {
            answers: vec![answer],
            asked: Vec::new(),
        };
        run(
            &mut transport,
            &[],
            &["en".into()],
            &mut layer,
            &path,
            &asked(false),
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(transport.asked.len(), 1, "failure remains retryable");
    }
    let mut transport = Canned {
        answers: vec![Ok(format!(
            r#"{{"id":"{id}","name":"Example","relations":[]}}"#
        ))],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &path,
        &asked(false),
        None,
    )
    .unwrap();
    let saved = sources::load(&path).unwrap().unwrap();
    let Facts::Artist(facts) = &saved.records[0].facts else {
        panic!("artist");
    };
    assert!(facts.relationships_complete);
    let mut repeat = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(
        &mut repeat,
        &[],
        &["en".into()],
        &mut layer,
        &path,
        &asked(false),
        None,
    )
    .unwrap();
    assert!(repeat.asked.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_merged_artist_link_lookup_keeps_the_reviewed_alias_and_its_confidence() {
    let dir = sandbox("merged_artist_links");
    let path = sources::sources_path(&dir);
    let id = "11111111-1111-4111-8111-111111111111";
    let mut layer = held(None);
    layer.records[0].source_id = Some(id.into());
    layer.records[0].confidence = Confidence::Matched(95);
    let catalog = aede_core::model::Catalog::default();
    let selector = layer.review_items(&catalog)[0].id.clone();
    layer
        .decide(&catalog, &selector, sources::ReviewDecision::Accepted, 2)
        .unwrap();
    let reviews = layer.reviews.clone();
    let mut transport = Canned {
        answers: vec![Ok(
            r#"{"id":"22222222-2222-4222-8222-222222222222","name":"Example","relations":[]}"#
                .into(),
        )],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &path,
        &asked(false),
        Some(&catalog),
    )
    .unwrap();
    assert_eq!(transport.asked.len(), 1);
    assert!(transport.asked[0].contains(id));
    let saved = sources::load_all(&path).unwrap().unwrap();
    let identity = saved.get(&entity(), sources::MUSICBRAINZ).unwrap();
    assert_eq!(identity.source_id.as_deref(), Some(id));
    assert_eq!(identity.confidence, Confidence::Matched(95));
    assert_eq!(saved.reviews, reviews);
    assert!(matches!(&identity.facts,Facts::Artist(facts) if facts.relationships_complete));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_biography_cached_for_another_wikidata_identity_cannot_suppress_the_validated_match() {
    let dir = sandbox("changed_wikidata_identity");
    let path = sources::sources_path(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    layer.set(SourceRecord {
        key: entity().key,
        source: wikipedia::SOURCE.into(),
        source_id: Some("Q999".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            summary: Some(Prose {
                text: "Biography for a previous identity.".into(),
                url: "https://en.wikipedia.org/wiki/Previous_identity".into(),
                lang: "en".into(),
                licence: wikipedia::LICENCE.into(),
            }),
            ..Default::default()
        }),
    });
    let mut transport = Canned {
        answers: vec![Ok(ENTITY.into()), Ok(SUMMARY.into())],
        asked: Vec::new(),
    };
    run(
        &mut transport,
        &[],
        &["en".into()],
        &mut layer,
        &path,
        &asked(false),
        None,
    )
    .unwrap();
    assert_eq!(
        transport.asked.len(),
        2,
        "a cached article for another identity is not a completed answer about the validated identity"
    );
    assert_eq!(stored(&layer).unwrap().text, "An American rock band.");
    assert_eq!(
        layer
            .get(&entity(), wikipedia::SOURCE)
            .unwrap()
            .source_id
            .as_deref(),
        Some("Q11649")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_interrupted_biography_refresh_keeps_saved_links_without_showing_the_old_identity() {
    let dir = sandbox("interrupted_biography_refresh");
    let path = sources::sources_path(&dir);
    let mut layer = held(None);
    let mbid = "11111111-1111-4111-8111-111111111111";
    layer.records[0].source_id = Some(mbid.into());
    layer.set(SourceRecord {
        key: entity().key,
        source: wikipedia::SOURCE.into(),
        source_id: Some("Q999".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            summary: Some(Prose {
                text: "Retained old biography.".into(),
                url: "https://en.wikipedia.org/wiki/Previous_identity".into(),
                lang: "en".into(),
                licence: wikipedia::LICENCE.into(),
            }),
            ..Default::default()
        }),
    });
    let old = layer.get(&entity(), wikipedia::SOURCE).unwrap().clone();
    let mut transport = Canned {
        answers: vec![
            Ok(format!(
                r#"{{"id":"{mbid}","name":"Example","relations":[{{"type":"wikidata","url":{{"resource":"https://www.wikidata.org/wiki/Q11649"}}}}]}}"#
            )),
            Err(super::super::fetch::Refusal::TooManyRequests),
        ],
        asked: Vec::new(),
    };
    assert!(
        run(
            &mut transport,
            &[],
            &["en".into()],
            &mut layer,
            &path,
            &asked(false),
            None
        )
        .is_err()
    );
    let saved = sources::load(&path).unwrap().unwrap();
    let previous = saved.get(&entity(), wikipedia::SOURCE).unwrap();
    assert_eq!(
        previous, &old,
        "an interrupted retrieval cannot erase its retained article"
    );
    assert!(!saved.is_trusted(&aede_core::model::Catalog::default(), previous));
    let Facts::Artist(current) = &saved.records[0].facts else {
        panic!("artist");
    };
    assert_eq!(
        current.wikidata.as_deref(),
        Some("https://www.wikidata.org/wiki/Q11649")
    );
    assert_eq!(
        transport.asked.len(),
        2,
        "stop after the provider's explicit refusal"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
