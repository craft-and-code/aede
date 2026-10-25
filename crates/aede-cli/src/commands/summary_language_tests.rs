//! Language refreshes use verified provider fixtures; negative variants remove links explicitly.
use super::*;
use aede_core::json::{Json, parse};
use aede_core::model::EntityKind;
use aede_core::sources::{ArtistFacts, Confidence, LabelFacts, Prose, Sources};
use std::collections::VecDeque;

const ENTITY: &str = include_str!("../../tests/fixtures/wikipedia/nirvana-entity.json");
const FRENCH: &str = include_str!("../../tests/fixtures/wikipedia/nirvana-fr.json");
const ENGLISH: &str = include_str!("../../tests/fixtures/wikipedia/nirvana-en.json");

struct Answers {
    documents: VecDeque<Json>,
    requested: Vec<String>,
}
impl Ask for Answers {
    fn get_bytes(&mut self, _: &str) -> Result<Vec<u8>, super::super::fetch::Refusal> {
        Err(super::super::fetch::Refusal::Failed(
            "summary fixtures never download images".into(),
        ))
    }
    fn get_json(&mut self, url: &str) -> Result<Json, super::super::fetch::Refusal> {
        self.requested.push(url.into());
        self.documents
            .pop_front()
            .ok_or_else(|| super::super::fetch::Refusal::Failed("fixture transport failure".into()))
    }
}
fn transport(documents: &[&str]) -> Answers {
    Answers {
        documents: documents.iter().map(|doc| parse(doc).unwrap()).collect(),
        requested: Vec::new(),
    }
}
fn entity(kind: EntityKind) -> EntityRef {
    EntityRef {
        kind,
        key: "language fixture".into(),
    }
}
fn facts(kind: EntityKind, wikidata: Option<String>, summary: Option<Prose>) -> Facts {
    match kind {
        EntityKind::Artist => Facts::Artist(ArtistFacts {
            wikidata,
            summary,
            ..Default::default()
        }),
        EntityKind::Label => Facts::Label(LabelFacts {
            wikidata,
            summary,
            ..Default::default()
        }),
        _ => unreachable!(),
    }
}
fn summary(layer: &Sources, kind: EntityKind) -> Option<&Prose> {
    match &layer.get(&entity(kind), wikipedia::SOURCE)?.facts {
        Facts::Artist(facts) => facts.summary.as_ref(),
        Facts::Label(facts) => facts.summary.as_ref(),
        _ => None,
    }
}
fn initial(kind: EntityKind, language: Option<&str>) -> Sources {
    let mut layer = Sources::default();
    layer.set(SourceRecord {
        key: entity(kind).key,
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("11111111-1111-4111-8111-111111111111".into()),
        confidence: Confidence::Identified,
        fetched_at: 1,
        facts: facts(
            kind,
            Some("https://www.wikidata.org/wiki/Q11649".into()),
            None,
        ),
    });
    if let Some(language) = language {
        let doc = parse(if language == "fr" { FRENCH } else { ENGLISH }).unwrap();
        let article = wikipedia::article(&parse(ENTITY).unwrap(), "Q11649", &[language]).unwrap();
        layer.set(SourceRecord {
            key: entity(kind).key,
            source: wikipedia::SOURCE.into(),
            source_id: Some("Q11649".into()),
            confidence: Confidence::Identified,
            fetched_at: 1,
            facts: facts(kind, None, wikipedia::prose(&doc, &article)),
        });
    }
    layer
}
fn exercise(
    layer: &mut Sources,
    kind: EntityKind,
    langs: &[String],
    responses: &mut Answers,
) -> std::path::PathBuf {
    let name = std::thread::current().name().unwrap().replace("::", "_");
    let dir = std::env::temp_dir().join(format!("aede_summary_language_{name}_{kind:?}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = sources::sources_path(&dir);
    sources::save(layer, &path).unwrap();
    let mut asked = super::tests::asked(false);
    asked.langs = langs.to_vec();
    run(responses, &[], langs, layer, &path, &asked, None).unwrap();
    *layer = sources::load_all(&path).unwrap().unwrap();
    dir
}
#[test]
fn legacy_english_artist_and_label_biographies_are_refetched_in_french_without_full() {
    for kind in [EntityKind::Artist, EntityKind::Label] {
        let mut layer = initial(kind, Some("en"));
        let mut responses = transport(&[ENTITY, FRENCH]);
        let dir = exercise(
            &mut layer,
            kind,
            &["fr".into(), "en".into()],
            &mut responses,
        );
        assert_eq!(responses.requested.len(), 2);
        assert!(responses.requested[1].starts_with("https://fr.wikipedia.org/"));
        let prose = summary(&layer, kind).unwrap();
        assert_eq!(prose.lang, "fr");
        assert_eq!(prose.licence, wikipedia::LICENCE);
        assert!(prose.url.starts_with("https://fr.wikipedia.org/"));
        let mut unchanged = transport(&[]);
        let again = exercise(
            &mut layer,
            kind,
            &["fr".into(), "en".into()],
            &mut unchanged,
        );
        assert!(unchanged.requested.is_empty());
        std::fs::remove_dir_all(&again).unwrap();
        assert_eq!(again, dir);
    }
}
#[test]
fn a_failed_language_refresh_keeps_the_old_attributed_biography_retryable() {
    for kind in [EntityKind::Artist, EntityKind::Label] {
        let mut layer = initial(kind, Some("en"));
        let previous = layer.clone();
        let mut failed = transport(&[]);
        let dir = exercise(&mut layer, kind, &["fr".into(), "en".into()], &mut failed);
        assert_eq!(failed.requested.len(), 1);
        assert_eq!(layer, previous);
        let mut retry = transport(&[ENTITY, FRENCH]);
        exercise(&mut layer, kind, &["fr".into(), "en".into()], &mut retry);
        assert_eq!(summary(&layer, kind).unwrap().lang, "fr");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn legacy_biographies_already_in_the_preferred_language_need_no_request() {
    for kind in [EntityKind::Artist, EntityKind::Label] {
        let mut layer = initial(kind, Some("fr"));
        let previous = layer.clone();
        let mut responses = transport(&[]);
        let dir = exercise(
            &mut layer,
            kind,
            &["fr".into(), "en".into()],
            &mut responses,
        );
        assert!(responses.requested.is_empty());
        assert_eq!(layer, previous);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

/// Deliberately remove French (and optionally all) links from the verified entity response.
fn missing_links(keep_english: bool) -> String {
    let original = parse(ENTITY).unwrap();
    let original = original.get("entities").unwrap().get("Q11649").unwrap();
    let mut links = Json::obj();
    if keep_english {
        links.set(
            "enwiki",
            original
                .get("sitelinks")
                .unwrap()
                .get("enwiki")
                .unwrap()
                .clone(),
        );
    }
    let mut item = Json::obj();
    item.set("id", "Q11649".into());
    item.set("sitelinks", links);
    let mut entities = Json::obj();
    entities.set("Q11649", item);
    let mut root = Json::obj();
    root.set("entities", entities);
    root.to_string_compact()
}

#[test]
fn fallback_and_absence_are_cached_only_for_the_completed_language_preference() {
    for kind in [EntityKind::Artist, EntityKind::Label] {
        for fallback in [true, false] {
            let mut layer = initial(kind, None);
            let response = missing_links(fallback);
            let documents = if fallback {
                vec![response.as_str(), ENGLISH]
            } else {
                vec![response.as_str()]
            };
            let mut answers = transport(&documents);
            let dir = exercise(&mut layer, kind, &["fr".into(), "en".into()], &mut answers);
            assert_eq!(answers.requested.len(), if fallback { 2 } else { 1 });
            assert_eq!(
                summary(&layer, kind).map(|prose| prose.lang.as_str()),
                fallback.then_some("en")
            );
            let previous = layer.clone();
            let mut repeated = transport(&[]);
            exercise(&mut layer, kind, &["fr".into(), "en".into()], &mut repeated);
            assert!(
                repeated.requested.is_empty(),
                "a confirmed fallback or absence is not queried endlessly"
            );
            assert_eq!(layer, previous);
            let mut changed = transport(&[ENTITY, FRENCH]);
            exercise(&mut layer, kind, &["es".into(), "fr".into()], &mut changed);
            assert_eq!(
                changed.requested.len(),
                2,
                "another preference needs its own completed answer"
            );
            assert_eq!(summary(&layer, kind).unwrap().lang, "fr");
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn legacy_empty_biographies_do_not_claim_that_french_was_already_requested() {
    for kind in [EntityKind::Artist, EntityKind::Label] {
        let mut layer = initial(kind, Some("en"));
        let mut previous = layer.get(&entity(kind), wikipedia::SOURCE).unwrap().clone();
        previous.facts = facts(kind, None, None);
        layer.set(previous);
        let mut responses = transport(&[ENTITY, FRENCH]);
        let dir = exercise(
            &mut layer,
            kind,
            &["fr".into(), "en".into()],
            &mut responses,
        );
        assert_eq!(responses.requested.len(), 2);
        assert_eq!(summary(&layer, kind).unwrap().lang, "fr");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn a_response_in_another_language_cannot_complete_the_preferred_article() {
    let kind = EntityKind::Artist;
    let mut layer = initial(kind, Some("en"));
    let previous = layer.clone();
    let mut answers = transport(&[ENTITY, ENGLISH]);
    let dir = exercise(&mut layer, kind, &["fr".into(), "en".into()], &mut answers);
    assert_eq!(answers.requested.len(), 2);
    assert_eq!(
        layer, previous,
        "a wrong-language response must retain the old record and remain retryable"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_language_change_does_not_bypass_an_explicit_biography_rejection() {
    let kind = EntityKind::Artist;
    let mut layer = initial(kind, Some("en"));
    layer.set_review(sources::SourceReview {
        entity: entity(kind),
        source: wikipedia::SOURCE.into(),
        source_id: Some("Q11649".into()),
        decision: sources::ReviewDecision::Rejected,
        reviewed_at: 2,
    });
    let previous = layer.clone();
    let mut responses = transport(&[]);
    let dir = exercise(
        &mut layer,
        kind,
        &["fr".into(), "en".into()],
        &mut responses,
    );
    assert!(responses.requested.is_empty());
    assert_eq!(layer, previous);
    std::fs::remove_dir_all(dir).unwrap();
}
