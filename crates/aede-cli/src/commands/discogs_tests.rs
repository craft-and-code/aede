use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, Prose};
use aede_core::tags::RawTags;

#[test]
fn discogs_link_completion_keeps_the_confirmed_canonical_label_identity() {
    let original = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    let mut catalog = catalog();
    catalog.labels[0].mbid = Some(original.into());
    let entity = entity(&catalog);
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some(original.into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    let target = path("canonical_label_link");
    refresh(&catalog, catalog.labels[0].id, &mut held, &target,
        &mut |_| aede_core::json::parse(&format!(r#"{{"id":"{canonical}","name":"Roadracer Records","relations":[{{"type":"discogs","url":{{"resource":"https://www.discogs.com/label/33088"}}}}]}}"#)).map_err(|error| error.to_string()),
        &mut |_| aede_core::json::parse(r#"{"id":33088,"name":"Roadracer Records","profile":"A label biography."}"#).map_err(|error| error.to_string())
    ).unwrap();
    let saved = sources::load_all(&target).unwrap().unwrap();
    let identity = saved.get(&entity, sources::MUSICBRAINZ).unwrap();
    assert!(saved.is_trusted(&catalog, identity));
    catalog.labels[0].mbid = Some(canonical.into());
    assert!(
        saved.is_trusted(&catalog, identity),
        "retagging must recognize the canonical label confirmed during completion"
    );
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}

fn catalog() -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("artist", "Sepultura");
    tags.insert("albumartist", "Sepultura");
    tags.insert("album", "Beneath the Remains");
    tags.insert("title", "Inner Self");
    tags.insert("label", "Roadracer Records");
    build(
        vec![ScannedFile {
            path: "/music/Sepultura/01.flac".to_string(),
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

fn entity(catalog: &Catalog) -> EntityRef {
    EntityRef::of(catalog, EntityKind::Label, catalog.labels[0].id).expect("label")
}

fn path(name: &str) -> std::path::PathBuf {
    let owner = std::thread::current()
        .name()
        .unwrap_or("test")
        .replace("::", "_");
    let dir = std::env::temp_dir().join(format!("aede_discogs_{owner}_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test directory");
    sources::sources_path(&dir)
}

fn musicbrainz(held: &mut Sources, entity: &EntityRef) {
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("ce105941-dc16-4291-9d18-5fed7edc5e5a".to_string()),
        fetched_at: 1,
        confidence: Confidence::matched(99),
        facts: Facts::Label(LabelFacts::default()),
    });
    held.set_review(sources::SourceReview {
        entity: entity.clone(),
        source: sources::MUSICBRAINZ.to_owned(),
        source_id: held
            .records
            .last()
            .and_then(|record| record.source_id.clone()),
        decision: sources::ReviewDecision::Accepted,
        reviewed_at: 2,
    });
}

#[test]
fn pending_and_rejected_label_identities_cannot_supply_a_discogs_profile() {
    let catalog = catalog();
    let entity = entity(&catalog);
    for rejected in [false, true] {
        let mut held = Sources::default();
        musicbrainz(&mut held, &entity);
        held.reviews.clear();
        if let Facts::Label(facts) = &mut held.records[0].facts {
            facts.discogs = Some("https://www.discogs.com/label/33088".into());
        }
        if rejected {
            let selector = held.review_items(&catalog)[0].id.clone();
            held.decide(&catalog, &selector, sources::ReviewDecision::Rejected, 2)
                .expect("reject identity");
        }
        let path = path(if rejected { "rejected" } else { "pending" });
        sources::save(&held, &path).expect("initial layer");
        let before = std::fs::read(&path).expect("source bytes");
        let mut calls = Vec::new();
        let result = refresh(
            &catalog,
            catalog.labels[0].id,
            &mut held,
            &path,
            &mut |_| panic!("an untrusted identity must not issue a MusicBrainz lookup"),
            &mut |url| {
                calls.push(url.to_owned());
                aede_core::json::parse(
                    r#"{"id":33088,"name":"Roadracer Records","profile":"An unrelated profile."}"#,
                )
                .map_err(|error| error.to_string())
            },
        );
        assert!(result.is_err(), "untrusted identity must be reviewed first");
        assert!(
            calls.is_empty(),
            "untrusted identity issued requests: {calls:?}"
        );
        assert_eq!(std::fs::read(&path).expect("source bytes"), before);
        assert!(held.get(&entity, discogs::SOURCE).is_none());
    }
}

#[test]
fn a_missing_wikipedia_bio_uses_the_musicbrainz_discogs_link_and_saves_the_profile() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    let path = path("fallback");
    let mut mb_calls = Vec::new();
    let mut dc_calls = Vec::new();
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut |url| {
            mb_calls.push(url.to_string());
            aede_core::json::parse(
                r#"{"id":"ce105941-dc16-4291-9d18-5fed7edc5e5a","name":"Roadracer Records","relations":[{"type":"discogs","url":{"resource":"https://www.discogs.com/label/33088"}}]}"#,
            )
            .map_err(|error| error.to_string())
        },
        &mut |url| {
            dc_calls.push(url.to_string());
            aede_core::json::parse(
                r#"{"id":33088,"name":"Roadracer Records","profile":"Label code: LC 9321."}"#,
            )
            .map_err(|error| error.to_string())
        },
    )
    .expect("fallback");
    assert_eq!(mb_calls.len(), 1);
    assert!(mb_calls[0].contains("inc=url-rels"));
    assert_eq!(dc_calls, vec!["https://api.discogs.com/labels/33088"]);
    let saved = sources::load(&path).expect("readable").expect("a layer");
    let record = saved.get(&entity, discogs::SOURCE).expect("Discogs record");
    let Facts::Label(facts) = &record.facts else {
        panic!("label facts");
    };
    assert_eq!(
        facts.summary.as_ref().map(|prose| prose.text.as_str()),
        Some("Label code: LC 9321.")
    );
    assert_eq!(
        facts.summary.as_ref().map(|prose| prose.url.as_str()),
        Some("https://www.discogs.com/label/33088")
    );
    assert_eq!(record.confidence, Confidence::Identified);
}

#[test]
fn an_accepted_musicbrainz_match_leads_to_a_trusted_identifier_linked_discogs_profile() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    if let Facts::Label(facts) = &mut held.records[0].facts {
        facts.discogs = Some("https://www.discogs.com/label/33088".into());
    }
    let previous = held.get(&entity, sources::MUSICBRAINZ).unwrap().clone();
    let reviews = held.reviews.clone();
    let path = path("accepted_lookup");
    let mut answer = |url: &str| {
        assert_eq!(url, "https://api.discogs.com/labels/33088");
        aede_core::json::parse(
            r#"{"id":33088,"name":"Roadracer Records","profile":"A verified label profile."}"#,
        )
        .map_err(|error| error.to_string())
    };
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut |_| panic!("the trusted label already names its Discogs ID"),
        &mut answer,
    )
    .unwrap();
    let saved = sources::load(&path).unwrap().unwrap();
    let profile = saved.get(&entity, discogs::SOURCE).unwrap();
    assert!(
        saved.is_trusted(&catalog, profile),
        "an exact linked lookup must be displayable after the parent identity is accepted"
    );
    assert_eq!(saved.get(&entity, sources::MUSICBRAINZ), Some(&previous));
    assert_eq!(
        saved.reviews, reviews,
        "approval is not copied to a different source"
    );
    assert_eq!(previous.confidence, Confidence::matched(99));
    held.set_review(sources::SourceReview {
        entity: entity.clone(),
        source: discogs::SOURCE.into(),
        source_id: Some("33088".into()),
        decision: sources::ReviewDecision::Rejected,
        reviewed_at: 3,
    });
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut |_| panic!("the trusted label already names its Discogs ID"),
        &mut answer,
    )
    .unwrap();
    let saved = sources::load(&path).unwrap().unwrap();
    let profile = saved.get(&entity, discogs::SOURCE).unwrap();
    assert!(
        !saved.is_trusted(&catalog, profile),
        "an explicit Discogs rejection still wins"
    );
    assert_eq!(saved.get(&entity, sources::MUSICBRAINZ), Some(&previous));
}

#[test]
fn wikipedia_avoids_discogs_but_a_current_discogs_profile_is_checked_again() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let path = path("already");
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: "wikipedia".to_string(),
        source_id: Some("Q1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            summary: Some(Prose {
                text: "A label.".to_string(),
                url: "https://en.wikipedia.org/wiki/A_label".to_string(),
                lang: "en".to_string(),
                licence: "CC BY-SA 4.0".to_string(),
            }),
            ..Default::default()
        }),
    });
    let mut no_mb =
        |_url: &str| -> Result<Json, String> { panic!("no MusicBrainz request needed") };
    let mut no_dc = |_url: &str| -> Result<Json, String> { panic!("no Discogs request needed") };
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut no_mb,
        &mut no_dc,
    )
    .expect("Wikipedia wins");

    held.records.retain(|record| record.source != "wikipedia");
    held.set(SourceRecord {
        key: entity.key,
        source: discogs::SOURCE.to_string(),
        source_id: Some("33088".to_string()),
        fetched_at: clock::now_seconds(),
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    if let Some(record) = held
        .records
        .iter_mut()
        .find(|record| record.source == sources::MUSICBRAINZ)
        && let Facts::Label(facts) = &mut record.facts
    {
        facts.discogs = Some("https://www.discogs.com/label/33088".to_string());
    }
    let mut calls = 0;
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut no_mb,
        &mut |_url| {
            calls += 1;
            aede_core::json::parse(
                r#"{"id":33088,"name":"Roadracer Records","profile":"Updated profile."}"#,
            )
            .map_err(|error| error.to_string())
        },
    )
    .expect("fresh Discogs answer is checked again");
    assert_eq!(calls, 1);
}

#[test]
fn linked_labels_are_resolved_before_the_profile_is_saved() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    if let Some(record) = held.records.first_mut()
        && let Facts::Label(facts) = &mut record.facts
    {
        facts.discogs = Some("https://www.discogs.com/label/33088".into());
    }
    let path = path("linked_labels");
    let mut asked = Vec::new();
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut |_| panic!("MusicBrainz link is already held"),
        &mut |url| {
            asked.push(url.to_string());
            let payload = match url {
                "https://api.discogs.com/labels/33088" => r#"{"id":33088,"name":"Roadracer Records","profile":"Use [l30552] and [l261823]. [b]Label Code[/b]."}"#,
                "https://api.discogs.com/labels/30552" => r#"{"id":30552,"name":"First Label"}"#,
                "https://api.discogs.com/labels/261823" => r#"{"id":261823,"name":"Second Label"}"#,
                _ => panic!("unexpected request: {url}"),
            };
            aede_core::json::parse(payload).map_err(|error| error.to_string())
        },
    )
    .expect("resolved profile");
    assert_eq!(asked.len(), 3);
    let saved = sources::load_all(&path).expect("readable").expect("layer");
    let row = saved.get(&entity, discogs::SOURCE).expect("profile");
    let Facts::Label(facts) = &row.facts else {
        panic!("label facts")
    };
    assert_eq!(
        facts.summary.as_ref().map(|summary| summary.text.as_str()),
        Some("Use First Label and Second Label. Label Code.")
    );
    assert_eq!(
        facts.summary_markup.as_deref(),
        Some("Use [l30552] and [l261823]. [b]Label Code[/b].")
    );

    let mut repeat_calls = 0;
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut |_| panic!("MusicBrainz link is already held"),
        &mut |url| {
            repeat_calls += 1;
            assert_eq!(url, "https://api.discogs.com/labels/33088");
            aede_core::json::parse(
                r#"{"id":33088,"name":"Roadracer Records","profile":"Use [l30552] and [l261823]. [b]Label Code[/b]."}"#,
            )
            .map_err(|error| error.to_string())
        },
    )
    .expect("unchanged profile");
    assert_eq!(repeat_calls, 1, "stored names are reused");
}

#[test]
fn an_unchanged_older_roadrunner_profile_resolves_artist_codes_on_refresh() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    if let Some(record) = held.records.first_mut()
        && let Facts::Label(facts) = &mut record.facts
    {
        facts.discogs = Some("https://www.discogs.com/label/33088".into());
    }
    let markup = "Founded by [a1258936].";
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: discogs::SOURCE.to_string(),
        source_id: Some("33088".into()),
        fetched_at: clock::now_seconds(),
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            summary: Some(Prose {
                text: markup.into(),
                url: discogs::label_url(33088),
                lang: "und".into(),
                licence: "CC0".into(),
            }),
            summary_markup: Some(markup.into()),
            ..Default::default()
        }),
    });
    let path = path("old_artist_codes");
    let mut asked = Vec::new();
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &path,
        &mut |_| panic!("MusicBrainz link is already held"),
        &mut |url| {
            asked.push(url.to_string());
            let payload = match url {
                "https://api.discogs.com/labels/33088" => {
                    r#"{"id":33088,"name":"Roadracer Records","profile":"Founded by [a1258936]."}"#
                }
                "https://api.discogs.com/artists/1258936" => r#"{"id":1258936,"name":"Founder"}"#,
                _ => panic!("unexpected request: {url}"),
            };
            aede_core::json::parse(payload).map_err(|error| error.to_string())
        },
    )
    .expect("profile refreshed");
    assert_eq!(asked.len(), 2);
    let row = held.get(&entity, discogs::SOURCE).expect("profile");
    let Facts::Label(facts) = &row.facts else {
        panic!("label facts")
    };
    assert_eq!(
        facts.summary.as_ref().map(|prose| prose.text.as_str()),
        Some("Founded by Founder.")
    );
}

#[test]
fn fetch_discogs_is_explicit_scoped_dry_and_reuses_fresh_profiles() {
    let c = catalog();
    let entity = entity(&c);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    let mut asked = super::super::fetch::Asked {
        names: &[],
        scope: &super::super::fetch::EVERYTHING,
        again: false,
        dry_run: true,
        size: super::super::covers::DEFAULT_SIZE,
        size_requested: false,
        images: false,
        fanart: Default::default(),
        langs: vec!["en".into()],
        key: None,
        portrait_key: None,
    };
    let p = path("batch_dry");
    run(&c, &mut Never, &[], &mut held, &p, &asked).unwrap();
    assert!(!p.exists());
    asked.dry_run = false;
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: discogs::SOURCE.into(),
        source_id: Some("33088".into()),
        fetched_at: clock::now_seconds(),
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            summary: Some(Prose {
                text: "Stored".into(),
                url: discogs::label_url(33088),
                lang: "en".into(),
                licence: "CC0".into(),
            }),
            ..Default::default()
        }),
    });
    run(&c, &mut Never, &[], &mut held, &p, &asked).unwrap();
    if let Facts::Label(facts) = &mut held.records[0].facts {
        facts.discogs = Some(discogs::label_url(33088));
    }
    held.set_review(sources::SourceReview {
        entity: entity.clone(),
        source: discogs::SOURCE.into(),
        source_id: Some("33088".into()),
        decision: sources::ReviewDecision::Rejected,
        reviewed_at: 3,
    });
    // A review decision alone must not force another request for the same ID.
    run(&c, &mut Never, &[], &mut held, &p, &asked).unwrap();
    let other = ["Other label".into()];
    asked.names = &other;
    asked.again = true;
    run(&c, &mut Never, &[], &mut held, &p, &asked).unwrap();
    assert!(!p.exists());
}
struct Never;
impl super::super::fetch::Ask for Never {
    fn get_json(&mut self, _: &str) -> Result<Json, super::super::fetch::Refusal> {
        panic!("no network expected")
    }
    fn get_bytes(&mut self, _: &str) -> Result<Vec<u8>, super::super::fetch::Refusal> {
        panic!("no images expected")
    }
}

struct BatchTransport;
impl super::super::fetch::Ask for BatchTransport {
    fn get_json(&mut self, url: &str) -> Result<Json, super::super::fetch::Refusal> {
        let value = if url.starts_with("https://api.discogs.com/") {
            r#"{"id":12345,"name":"Control Label","profile":"Original control biography."}"#
        } else if url.contains("11111111-1111-4111-8111-111111111111") {
            r#"{"id":"11111111-1111-4111-8111-111111111111","name":"Control Label","relations":[{"type":"discogs","url":{"resource":"https://www.discogs.com/label/12345"}}]}"#
        } else {
            r#"{"id":"ce105941-dc16-4291-9d18-5fed7edc5e5a","name":"Roadracer Records","relations":[]}"#
        };
        Ok(aede_core::json::parse(value).unwrap())
    }
    fn get_bytes(&mut self, _: &str) -> Result<Vec<u8>, super::super::fetch::Refusal> {
        Err(super::super::fetch::Refusal::Failed(
            "No image request expected".into(),
        ))
    }
}

#[test]
fn only_a_trusted_wikipedia_biography_can_suppress_explicit_discogs_fetching() {
    let mut catalog = catalog();
    catalog.labels[0].name = "Control Label".into();
    catalog.labels[0].key = "control label".into();
    let entity = entity(&catalog);
    for mode in ["pending", "rejected", "accepted", "identified"] {
        let mut original = Sources::default();
        musicbrainz(&mut original, &entity);
        if let Facts::Label(facts) = &mut original.records[0].facts {
            facts.discogs = Some("https://www.discogs.com/label/12345".into());
        }
        let wiki = SourceRecord {
            key: entity.key.clone(),
            source: "wikipedia".into(),
            source_id: Some("Q123".into()),
            fetched_at: 1,
            confidence: if mode == "identified" {
                Confidence::Identified
            } else {
                Confidence::matched(80)
            },
            facts: Facts::Label(LabelFacts {
                summary: Some(Prose {
                    text: "An unconfirmed biography.".into(),
                    url: "https://en.wikipedia.org/wiki/Control_Label".into(),
                    lang: "en".into(),
                    licence: "CC BY-SA 4.0".into(),
                }),
                ..Default::default()
            }),
        };
        original.set(wiki.clone());
        if mode == "rejected" || mode == "accepted" {
            let selector = original
                .review_items(&catalog)
                .into_iter()
                .find(|review| review.source == "wikipedia")
                .unwrap()
                .id;
            let decision = if mode == "accepted" {
                sources::ReviewDecision::Accepted
            } else {
                sources::ReviewDecision::Rejected
            };
            original.decide(&catalog, &selector, decision, 3).unwrap();
        }
        for batch in [false, true] {
            let mut held = original.clone();
            let path = path(&format!("wiki_{mode}_{batch}"));
            if batch {
                let asked = super::super::fetch::Asked {
                    names: &[],
                    scope: &super::super::fetch::EVERYTHING,
                    again: false,
                    dry_run: false,
                    size: super::super::covers::DEFAULT_SIZE,
                    size_requested: false,
                    images: false,
                    fanart: Default::default(),
                    langs: vec!["en".into()],
                    key: None,
                    portrait_key: None,
                };
                run(&catalog, &mut BatchTransport, &[], &mut held, &path, &asked).unwrap();
            } else {
                refresh(
                    &catalog,
                    catalog.labels[0].id,
                    &mut held,
                    &path,
                    &mut |_| panic!("trusted MusicBrainz link is already held"),
                    &mut |url| {
                        super::super::fetch::Ask::get_json(&mut BatchTransport, url)
                            .map_err(|why| why.to_string())
                    },
                )
                .unwrap();
            }
            assert_eq!(
                held.get(&entity, discogs::SOURCE).is_some(),
                !["identified", "accepted"].contains(&mode),
                "{mode}, batch={batch}"
            );
            assert_eq!(held.get(&entity, "wikipedia"), Some(&wiki));
        }
    }
}
#[test]
fn batch_retains_later_profiles_when_an_earlier_label_has_no_discogs_link() {
    let mut catalog = catalog();
    catalog.labels[0].mbid = Some("ce105941-dc16-4291-9d18-5fed7edc5e5a".into());
    let mut label = catalog.labels[0].clone();
    label.id = 1;
    label.key = "control label".into();
    label.name = "Control Label".into();
    label.mbid = Some("11111111-1111-4111-8111-111111111111".into());
    catalog.labels.push(label);
    let target = path("partial_batch");
    let mut held = Sources::default();
    let asked = super::super::fetch::Asked {
        names: &[],
        scope: &super::super::fetch::EVERYTHING,
        again: false,
        dry_run: false,
        size: aede_core::coverart::Size::Original,
        size_requested: false,
        images: false,
        fanart: Default::default(),
        langs: vec!["en".into()],
        key: None,
        portrait_key: None,
    };
    assert!(
        run(
            &catalog,
            &mut BatchTransport,
            &[],
            &mut held,
            &target,
            &asked
        )
        .is_err()
    );
    let saved = sources::load(&target).unwrap().unwrap();
    let entity = EntityRef::of(&catalog, EntityKind::Label, 1).unwrap();
    assert!(
        matches!(&saved.get(&entity, discogs::SOURCE).unwrap().facts, Facts::Label(f) if f.summary.as_ref().unwrap().text == "Original control biography.")
    );
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}

struct ThrottledTransport(usize);

#[test]
fn a_failed_source_publication_stops_the_batch_instead_of_becoming_an_unavailable_profile() {
    let mut catalog = catalog();
    catalog.labels[0].name = "Control Label".into();
    catalog.labels[0].key = "control label".into();
    catalog.labels[0].mbid = Some("11111111-1111-4111-8111-111111111111".into());
    let target = path("publication_failure");
    std::fs::create_dir(&target).unwrap();
    let asked = super::super::fetch::Asked {
        names: &[],
        scope: &super::super::fetch::EVERYTHING,
        again: false,
        dry_run: false,
        size: aede_core::coverart::Size::Original,
        size_requested: false,
        images: false,
        fanart: Default::default(),
        langs: vec!["en".into()],
        key: None,
        portrait_key: None,
    };
    let error = run(
        &catalog,
        &mut BatchTransport,
        &[],
        &mut Sources::default(),
        &target,
        &asked,
    )
    .unwrap_err();
    assert!(
        error.is::<aede_core::store::StoreError>(),
        "a storage failure must remain fatal: {error}"
    );
    assert!(target.is_dir());
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}

impl super::super::fetch::Ask for ThrottledTransport {
    fn get_json(&mut self, _: &str) -> Result<Json, super::super::fetch::Refusal> {
        self.0 += 1;
        Err(super::super::fetch::Refusal::TooManyRequests)
    }
    fn get_bytes(&mut self, _: &str) -> Result<Vec<u8>, super::super::fetch::Refusal> {
        unreachable!("No image request")
    }
}
#[test]
fn batch_stops_after_provider_throttling_without_requesting_other_labels() {
    let mut catalog = catalog();
    catalog.labels[0].mbid = Some("ce105941-dc16-4291-9d18-5fed7edc5e5a".into());
    let mut label = catalog.labels[0].clone();
    label.id = 1;
    label.key = "control label".into();
    label.name = "Control Label".into();
    catalog.labels.push(label);
    let target = path("throttled_batch");
    let mut held = Sources::default();
    let asked = super::super::fetch::Asked {
        names: &[],
        scope: &super::super::fetch::EVERYTHING,
        again: false,
        dry_run: false,
        size: aede_core::coverart::Size::Original,
        size_requested: false,
        images: false,
        fanart: Default::default(),
        langs: vec!["en".into()],
        key: None,
        portrait_key: None,
    };
    let mut transport = ThrottledTransport(0);
    assert!(run(&catalog, &mut transport, &[], &mut held, &target, &asked).is_err());
    assert_eq!(transport.0, 1);
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}

#[test]
fn a_fresh_profile_for_an_old_discogs_id_does_not_suppress_the_new_validated_label() {
    let mut c = catalog();
    c.labels[0].name = "Control Label".into();
    c.labels[0].key = "control label".into();
    let entity = entity(&c);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    if let Facts::Label(facts) = &mut held.records[0].facts {
        facts.discogs = Some(discogs::label_url(12345));
    }
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: discogs::SOURCE.into(),
        source_id: Some("99999".into()),
        fetched_at: clock::now_seconds(),
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            summary: Some(Prose {
                text: "A different label's biography.".into(),
                url: discogs::label_url(99999),
                lang: "en".into(),
                licence: "CC0".into(),
            }),
            ..Default::default()
        }),
    });
    let asked = super::super::fetch::Asked {
        names: &[],
        scope: &super::super::fetch::EVERYTHING,
        again: false,
        dry_run: false,
        size: super::super::covers::DEFAULT_SIZE,
        size_requested: false,
        images: false,
        fanart: Default::default(),
        langs: vec!["en".into()],
        key: None,
        portrait_key: None,
    };
    let p = path("changed_discogs_identity");
    run(&c, &mut BatchTransport, &[], &mut held, &p, &asked).unwrap();
    let current = held.get(&entity, discogs::SOURCE).unwrap();
    assert_eq!(
        current.source_id.as_deref(),
        Some("12345"),
        "cache freshness cannot prove that this is the validated label"
    );
    let Facts::Label(facts) = &current.facts else {
        panic!("label");
    };
    assert_eq!(
        facts.summary.as_ref().unwrap().text,
        "Original control biography."
    );
    std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
}

#[test]
fn a_merged_musicbrainz_label_keeps_its_review_and_can_supply_the_discogs_biography() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    let original = held.get(&entity, sources::MUSICBRAINZ).unwrap().clone();
    let mut reviews = held.reviews.clone();
    for review in &mut reviews {
        review.source_id = Some("22222222-2222-4222-8222-222222222222".into());
    }
    let target = path("merged_label");
    let mut calls = Vec::new();
    refresh(
        &catalog,
        catalog.labels[0].id,
        &mut held,
        &target,
        &mut |url| {
            assert!(url.contains(original.source_id.as_deref().unwrap()));
            aede_core::json::parse(
                r#"{"id":"22222222-2222-4222-8222-222222222222","name":"Roadracer Records","relations":[{"type":"discogs","url":{"resource":"https://www.discogs.com/label/33088"}}]}"#,
            ).map_err(|error| error.to_string())
        },
        &mut |url| {
            calls.push(url.to_owned());
            aede_core::json::parse(
                r#"{"id":33088,"name":"Roadracer Records","profile":"A label biography."}"#,
            ).map_err(|error| error.to_string())
        },
    ).expect("a merged MusicBrainz ID remains a valid official alias");
    assert_eq!(calls, ["https://api.discogs.com/labels/33088"]);
    let saved = sources::load_all(&target).unwrap().unwrap();
    let identity = saved.get(&entity, sources::MUSICBRAINZ).unwrap();
    assert_eq!(
        identity.source_id.as_deref(),
        Some("22222222-2222-4222-8222-222222222222")
    );
    assert_eq!(
        identity.musicbrainz_lookup_alias(),
        original.source_id.as_deref()
    );
    assert_eq!(identity.confidence, original.confidence);
    assert_eq!(saved.reviews, reviews);
    assert!(saved.get(&entity, discogs::SOURCE).is_some());
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}

#[test]
fn a_malformed_redirect_or_different_label_name_cannot_replace_the_reviewed_source() {
    let catalog = catalog();
    let entity = entity(&catalog);
    let mut held = Sources::default();
    musicbrainz(&mut held, &entity);
    let target = path("refused_label_redirect");
    sources::save(&held, &target).unwrap();
    let before = std::fs::read(&target).unwrap();
    for (id, name) in [
        ("../other", "Roadracer Records"),
        ("", "Roadracer Records"),
        ("22222222-2222-4222-8222-222222222222", "Another Label"),
    ] {
        let mut retained = held.clone();
        let answer = format!(
            r#"{{"id":"{id}","name":"{name}","relations":[{{"type":"discogs","url":{{"resource":"https://www.discogs.com/label/33088"}}}}]}}"#
        );
        assert!(
            refresh(
                &catalog,
                catalog.labels[0].id,
                &mut retained,
                &target,
                &mut |_| aede_core::json::parse(&answer).map_err(|error| error.to_string()),
                &mut |_| panic!("an unreadable or unconfirmed identity must not reach Discogs"),
            )
            .is_err()
        );
        assert_eq!(retained, held);
        assert_eq!(std::fs::read(&target).unwrap(), before);
    }
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}
