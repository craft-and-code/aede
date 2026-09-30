use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, Prose};
use aede_core::tags::RawTags;

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
    assert_eq!(record.confidence, Confidence::matched(99));
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
