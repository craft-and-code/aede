//! What `fetch` does, proved without a network.
//!
//! Split out of `fetch.rs`, which was half tests: the module is declared there
//! with `#[path]`, so these are still `fetch`'s own child module and still see
//! its private items through `use super::*`. Nothing about what they can reach
//! changed — only which file they sit in.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::RawTags;

#[test]
fn malformed_artist_and_label_refreshes_preserve_relationships_and_retry() {
    let dir = sandbox("artist_label_relations_admission");
    let audio = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    let track_path = dir.join("music").join("01.flac");
    std::fs::create_dir_all(track_path.parent().unwrap()).unwrap();
    std::fs::write(&track_path, audio).unwrap();
    let artist_id = "11111111-1111-4111-8111-111111111111";
    let label_id = "22222222-2222-4222-8222-222222222222";
    let mut tags = RawTags::default();
    for (key, value) in [
        ("artist", "Example Band"),
        ("albumartist", "Example Band"),
        ("album", "Example Album"),
        ("title", "Example Song"),
        ("musicbrainz_artistid", artist_id),
        ("musicbrainz_albumartistid", artist_id),
        ("label", "Example Records"),
        ("musicbrainz_labelid", label_id),
    ] {
        tags.insert(key, value);
    }
    let catalog = build(
        vec![ScannedFile {
            path: track_path.to_string_lossy().into_owned(),
            size: audio.len() as u64,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![dir.join("music").to_string_lossy().into_owned()],
        1,
        &[],
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
    let catalog_bytes = std::fs::read(aede_core::store::catalog_path(&dir)).unwrap();
    let path = sources::sources_path(&dir);
    let mut overwritten = Vec::new();
    for (kind, id, name, options) in [
        (
            EntityKind::Artist,
            artist_id,
            "Example Band",
            vec!["--full"],
        ),
        (
            EntityKind::Label,
            label_id,
            "Example Records",
            vec!["--labels", "--full"],
        ),
    ] {
        let entity = EntityRef::of(&catalog, kind, 0).unwrap();
        let valid = format!(
            r#"{{"id":"{id}","name":"{name}","relations":[{{"type":"wikidata","url":{{"resource":"https://www.wikidata.org/wiki/Q123"}}}}]}}"#
        );
        let facts = match kind {
            EntityKind::Artist => Facts::Artist(
                musicbrainz::artist(&aede_core::json::parse(&valid).unwrap())
                    .unwrap()
                    .facts,
            ),
            EntityKind::Label => Facts::Label(
                musicbrainz::label(&aede_core::json::parse(&valid).unwrap())
                    .unwrap()
                    .facts,
            ),
            _ => unreachable!(),
        };
        let mut malformed_answers: Vec<_> = ["null", "{}", "[null]"]
            .into_iter()
            .map(|relations| {
                (
                    format!("relations={relations}"),
                    format!(r#"{{"id":"{id}","name":"{name}","relations":{relations}}}"#),
                )
            })
            .collect();
        malformed_answers.push((
            "invalid identifier".into(),
            format!(r#"{{"id":"not-a-musicbrainz-id","name":"{name}","relations":[]}}"#),
        ));
        for (malformed, response) in malformed_answers {
            let mut held = sources::Sources::default();
            held.set(SourceRecord {
                key: entity.key.clone(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(id.into()),
                fetched_at: 1,
                confidence: sources::Confidence::Identified,
                facts: facts.clone(),
            });
            // Retained consent may predate a now-exact local attachment.
            held.set_review(sources::SourceReview {
                entity: entity.clone(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(id.into()),
                decision: sources::ReviewDecision::Accepted,
                reviewed_at: 1,
            });
            sources::save(&held, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let mut responses = vec![Ok(response)];
            if kind == EntityKind::Artist {
                responses.push(Ok(NO_ALBUM.into()));
            }
            let mut transport = Canned {
                answers: responses,
                asked: Vec::new(),
            };
            run_with(&args(&dir, &options), &mut transport, &NO_WAIT).unwrap();
            if std::fs::read(&path).unwrap() != before {
                overwritten.push(format!("{kind:?}: {malformed}"));
            }
            assert!(transport.answers.is_empty());

            let mut responses = vec![Ok(valid.clone())];
            if kind == EntityKind::Artist {
                responses.push(Ok(NO_ALBUM.into()));
            }
            let mut retry = Canned {
                answers: responses,
                asked: Vec::new(),
            };
            run_with(&args(&dir, &options), &mut retry, &NO_WAIT).unwrap();
            let restored = sources::load_all(&path).unwrap().unwrap();
            assert_eq!(
                restored.get(&entity, sources::MUSICBRAINZ).unwrap().facts,
                facts
            );
            assert_eq!(std::fs::read(&track_path).unwrap(), audio);
            assert_eq!(
                std::fs::read(aede_core::store::catalog_path(&dir)).unwrap(),
                catalog_bytes
            );
            assert!(!dir.join("user.json").exists());
            assert!(!dir.join("conclusions.json").exists());
        }
    }
    assert!(
        overwritten.is_empty(),
        "unreadable responses replaced prior facts: {overwritten:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unreachable_targets_get_one_later_attempt_and_other_refusals_do_not() {
    let targets = [1, 2];
    let mut pending = queue(&targets);
    let (first, retried) = pending.pop_front().expect("first target");
    assert!(defer(
        &mut pending,
        first,
        retried,
        &Refusal::Unreachable("timeout".into())
    ));
    assert_eq!(pending.pop_front(), Some((&2, false)));
    let (first, retried) = pending.pop_front().expect("deferred target");
    assert_eq!((*first, retried), (1, true));
    assert!(!defer(
        &mut pending,
        first,
        retried,
        &Refusal::Unreachable("timeout".into())
    ));
    assert!(!defer(&mut pending, first, false, &Refusal::RateLimited));
    assert!(pending.is_empty());
}

#[test]
fn artist_images_require_a_local_album_and_a_musicbrainz_identity() {
    let dir = sandbox("artist_image_candidates");
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let artist = catalog
        .artists
        .iter()
        .find(|artist| artist.name == "Miles Davis")
        .expect("album artist");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, artist.id).expect("artist reference");
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key,
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(Default::default()),
    });

    let candidates = artist_image_candidates(&catalog, &held, &[], &EVERYTHING);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].1.id, artist.id);
    assert_eq!(candidates[0].3, "mbid-1");
    assert!(artist_image_candidates(&catalog, &held, &["other".into()], &EVERYTHING).is_empty());

    held.records[0].source_id = None;
    assert!(artist_image_candidates(&catalog, &held, &[], &EVERYTHING).is_empty());
    held.records[0].source_id = Some("mbid-1".to_string());
    held.records[0].source = "not-musicbrainz".to_string();
    assert!(artist_image_candidates(&catalog, &held, &[], &EVERYTHING).is_empty());
    held.records[0].source = sources::MUSICBRAINZ.to_string();
    held.records[0].key = "artist no longer in the catalog".to_string();
    assert!(artist_image_candidates(&catalog, &held, &[], &EVERYTHING).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn artist_artwork_requires_an_exact_or_accepted_identity() {
    let dir = sandbox("artwork_trust");
    let catalog = super::super::load(&args(&dir, &[])).unwrap();
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).unwrap();
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("proposed-artist".into()),
        fetched_at: 1,
        confidence: sources::Confidence::Matched(95),
        facts: Facts::Artist(Default::default()),
    });
    assert!(
        artist_image_candidates(&catalog, &held, &[], &EVERYTHING).is_empty(),
        "an unreviewed match is evidence, not an artwork destination"
    );
    let id = held.review_items(&catalog)[0].id.clone();
    held.decide(&catalog, &id, sources::ReviewDecision::Accepted, 2)
        .unwrap();
    assert_eq!(
        artist_image_candidates(&catalog, &held, &[], &EVERYTHING).len(),
        1
    );
    held.decide(&catalog, &id, sources::ReviewDecision::Rejected, 3)
        .unwrap();
    assert!(artist_image_candidates(&catalog, &held, &[], &EVERYTHING).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

/// A transport that answers from canned text, and remembers what was asked.
struct Canned {
    answers: Vec<Result<String, Refusal>>,
    asked: Vec<String>,
}

impl Ask for Canned {
    fn get_json(&mut self, url: &str) -> Result<Json, Refusal> {
        self.asked.push(url.to_string());
        // Running out is a refusal, not a panic. A run asks about artists and
        // then about albums, so a test interested in only one half would
        // otherwise die three frames from anything it was asserting.
        if self.answers.is_empty() {
            return Err(Refusal::Failed("nothing canned for this".to_string()));
        }
        match self.answers.remove(0) {
            Ok(text) => Ok(aede_core::json::parse(&text).expect("valid fixture")),
            Err(why) => Err(why),
        }
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
        // These passes ask questions and never download anything; a call here
        // is a mistake worth failing on rather than a case worth answering.
        self.asked.push(url.to_string());
        Err(Refusal::Failed("this pass downloads nothing".to_string()))
    }
}

fn library(dir: &std::path::Path) -> std::path::PathBuf {
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Miles/Kind of Blue/01.flac".to_string(),
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
    );
    std::fs::create_dir_all(dir).expect("a data folder");
    let path = aede_core::store::catalog_path(dir);
    aede_core::store::save(&catalog, &path).expect("a catalog");
    path
}

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
    let dir = std::env::temp_dir().join(format!("aede_fetch_{}_{name}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    library(&dir);
    dir
}

fn args(dir: &std::path::Path, extra: &[&str]) -> Args {
    let mut raw = vec!["fetch".to_string(), format!("--data={}", dir.display())];
    raw.extend(extra.iter().map(|s| s.to_string()));
    Args::parse(raw)
}

fn accept_matches(dir: &std::path::Path) {
    let catalog = super::super::load(&args(dir, &[])).unwrap();
    let path = sources::sources_path(dir);
    let mut held = sources::load(&path).unwrap().unwrap();
    let ids = held
        .review_items(&catalog)
        .into_iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    for id in ids {
        held.decide(&catalog, &id, sources::ReviewDecision::Accepted, 2)
            .unwrap();
    }
    sources::save(&held, &path).unwrap();
}

#[test]
fn fanart_exclusions_select_each_image_family_independently() {
    let parsed = args(
        std::path::Path::new("/tmp"),
        &["--fanart", "--no-logo", "--no-background", "--no-cdart"],
    );
    let selected = FanartOptions::from_args(&parsed);

    assert!(selected.all);
    assert!(!selected.logo);
    assert!(selected.label_logo);
    assert!(selected.portrait);
    assert!(!selected.background);
    assert!(selected.banner);
    assert!(selected.album_cover);
    assert!(!selected.cdart);

    for (excluded, expected) in [
        ("--no-logo", [false, true, true, true, true, true, true]),
        (
            "--no-label-logo",
            [true, false, true, true, true, true, true],
        ),
        ("--no-portrait", [true, true, false, true, true, true, true]),
        (
            "--no-background",
            [true, true, true, false, true, true, true],
        ),
        ("--no-banner", [true, true, true, true, false, true, true]),
        (
            "--no-album-cover",
            [true, true, true, true, true, false, true],
        ),
        ("--no-cdart", [true, true, true, true, true, true, false]),
    ] {
        let parsed = args(std::path::Path::new("/tmp"), &["--fanart", excluded]);
        let selected = FanartOptions::from_args(&parsed);
        assert_eq!(
            [
                selected.logo,
                selected.label_logo,
                selected.portrait,
                selected.background,
                selected.banner,
                selected.album_cover,
                selected.cdart,
            ],
            expected,
            "{excluded} must disable only its own family"
        );
    }
}

#[test]
fn a_logo_pass_cannot_also_exclude_the_label_logos_it_requests() {
    let dir = sandbox("conflicting_label_logos");
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    let error = run_with(
        &args(
            &dir,
            &["--fanart", "--logos", "--no-label-logo", "--dry-run"],
        ),
        &mut transport,
        &NO_WAIT,
    )
    .expect_err("the same family was requested and excluded");
    assert!(
        error.to_string().contains("--logos and --no-label-logo"),
        "{error}"
    );
    assert!(transport.asked.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_artist_refresh_does_not_transfer_another_identitys_discography() {
    let dir = sandbox("changed_artist_identity");
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).expect("an artist");
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("previously-matched-artist".to_string()),
        fetched_at: 1,
        confidence: sources::Confidence::Matched(90),
        facts: Facts::Artist(sources::ArtistFacts {
            discography_fetched_at: Some(17),
            discography: vec![sources::KnownRelease {
                mbid: "unrelated-group".to_string(),
                title: "An Unrelated Album".to_string(),
                first_released: None,
                primary_type: None,
                secondary_types: Vec::new(),
            }],
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).expect("saved");
    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(NO_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--full"]), &mut transport, &NO_WAIT).expect("refresh");
    let after = sources::load(&sources::sources_path(&dir))
        .unwrap()
        .unwrap();
    let record = after.get(&entity, sources::MUSICBRAINZ).unwrap();
    assert_eq!(
        record.source_id.as_deref(),
        Some("561d854a-6a28-4aa7-8c99-323e6ce46c2a")
    );
    let Facts::Artist(facts) = &record.facts else {
        panic!("artist facts")
    };
    assert!(
        facts.discography.is_empty(),
        "another artist's albums must not be attributed to the new identity"
    );
    assert_eq!(
        facts.discography_fetched_at, None,
        "another identity's empty-answer cache must also be discarded"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_artist_refresh_preserves_a_complete_empty_discography_for_the_same_identity() {
    let dir = sandbox("keep_empty_discography");
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).expect("an artist");
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("561d854a-6a28-4aa7-8c99-323e6ce46c2a".into()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(sources::ArtistFacts {
            discography_fetched_at: Some(17),
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).unwrap();
    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.into()), Ok(ONE_ALBUM.into())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--full"]), &mut transport, &NO_WAIT).unwrap();
    let after = sources::load(&sources::sources_path(&dir))
        .unwrap()
        .unwrap();
    let Facts::Artist(facts) = &after.get(&entity, sources::MUSICBRAINZ).unwrap().facts else {
        panic!("artist facts");
    };
    assert!(facts.discography.is_empty());
    assert_eq!(
        facts.discography_fetched_at,
        Some(17),
        "the artist lookup never re-browsed release groups"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn redirected_artist_lookups_keep_trust_and_explicit_review_decisions() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    for decision in [
        None,
        Some(sources::ReviewDecision::Accepted),
        Some(sources::ReviewDecision::Rejected),
    ] {
        let dir = sandbox(&format!("redirect_artist_trust_{decision:?}"));
        let mut tags = RawTags::default();
        for (key, value) in [
            ("artist", "Example Band"),
            ("albumartist", "Example Band"),
            ("album", "Example Album"),
            ("title", "Example Song"),
            ("musicbrainz_artistid", queried),
            ("musicbrainz_albumartistid", queried),
        ] {
            tags.insert(key, value);
        }
        let catalog = build(
            vec![ScannedFile {
                path: dir.join("music/01.flac").to_string_lossy().into_owned(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            vec![dir.join("music").to_string_lossy().into_owned()],
            1,
            &[],
        );
        aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
        let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).unwrap();
        let mut held = sources::Sources::default();
        held.set(SourceRecord {
            key: entity.key.clone(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(queried.into()),
            fetched_at: 1,
            confidence: sources::Confidence::Matched(95),
            facts: Facts::Artist(sources::ArtistFacts::default()),
        });
        if let Some(decision) = decision {
            held.set_review(sources::SourceReview {
                entity: entity.clone(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(queried.into()),
                decision,
                reviewed_at: 17,
            });
        }
        let path = sources::sources_path(&dir);
        sources::save(&held, &path).unwrap();
        for canonical in [canonical, "33333333-3333-4333-8333-333333333333"] {
            let mut transport = Canned {
                answers: vec![
                    Ok(format!(
                        r#"{{"id":"{canonical}","name":"Example Band","relations":[
                {{"type":"wikidata","url":{{"resource":"https://www.wikidata.org/wiki/Q1001"}}}}
            ]}}"#
                    )),
                    Ok(NO_ALBUM.into()),
                ],
                asked: Vec::new(),
            };
            run_with(&args(&dir, &["--full"]), &mut transport, &NO_WAIT).unwrap();
            let saved = sources::load_all(&path).unwrap().unwrap();
            let record = saved.get(&entity, sources::MUSICBRAINZ).unwrap();
            assert_eq!(record.source_id.as_deref(), Some(canonical));
            let allowed = decision != Some(sources::ReviewDecision::Rejected);
            assert_eq!(
                saved.is_trusted(&catalog, record),
                allowed,
                "a valid redirect must not turn an exact lookup into an identity conflict"
            );
            let mut retagged = catalog.clone();
            retagged.artists[0].mbid = Some(canonical.into());
            let reloaded = sources::load_all(&path).unwrap().unwrap();
            let retagged_record = reloaded.get(&entity, sources::MUSICBRAINZ).unwrap();
            assert_eq!(
                reloaded.is_trusted(&retagged, retagged_record),
                allowed,
                "a saved artist claim must remain usable after canonical retagging"
            );
            assert_eq!(
                super::super::summaries::waiting(&reloaded, Some(&retagged), &["en".into()]),
                usize::from(allowed)
            );
            assert_eq!(
                saved
                    .review_for(record)
                    .map(|review| (review.decision, review.reviewed_at)),
                decision.map(|decision| (decision, 17)),
                "a redirect must not remove consent or rejection"
            );
            assert_eq!(
                super::super::summaries::waiting(&saved, Some(&catalog), &["en".into()]),
                usize::from(allowed)
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn an_artist_lookup_redirect_keeps_only_the_queried_identitys_discography() {
    let original = "11111111-1111-4111-8111-111111111111";
    let survivor = "22222222-2222-4222-8222-222222222222";
    let unrelated = "33333333-3333-4333-8333-333333333333";
    for (case, tagged, previous, preserve, captured_alias) in [
        ("merged lookup", true, original, true, false),
        ("unrelated prior claim", true, unrelated, false, false),
        ("changed search result", false, original, false, false),
        ("previously confirmed alias", true, unrelated, true, true),
    ] {
        for empty in [false, true] {
            let dir = sandbox(&format!("redirect_discography_{case}_{empty}"));
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Fixture Band"),
                ("albumartist", "Fixture Band"),
                ("album", "Fixture Album"),
                ("title", "Fixture Song"),
            ] {
                tags.insert(key, value);
            }
            if tagged {
                tags.insert("musicbrainz_artistid", original);
                tags.insert("musicbrainz_albumartistid", original);
            }
            let catalog = build(
                vec![ScannedFile {
                    path: dir.join("music/01.flac").to_string_lossy().into_owned(),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }],
                vec![dir.join("music").to_string_lossy().into_owned()],
                1,
                &[],
            );
            aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
            let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).unwrap();
            let known = if empty {
                Vec::new()
            } else {
                vec![sources::KnownRelease {
                    mbid: "44444444-4444-4444-8444-444444444444".into(),
                    title: "Fixture Missing Album".into(),
                    first_released: None,
                    primary_type: Some("Album".into()),
                    secondary_types: Vec::new(),
                }]
            };
            let mut held = sources::Sources::default();
            held.set(SourceRecord {
                key: entity.key.clone(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(previous.into()),
                fetched_at: 1,
                confidence: sources::Confidence::Identified,
                facts: Facts::Artist(sources::ArtistFacts {
                    lookup_mbid: captured_alias.then(|| original.to_string()),
                    discography: known.clone(),
                    discography_fetched_at: Some(17),
                    ..Default::default()
                }),
            });
            let path = sources::sources_path(&dir);
            sources::save(&held, &path).unwrap();
            let artist = format!(
                r#"{{"id":"{survivor}","name":"Fixture Band","score":100,"relations":[]}}"#
            );
            let response = if tagged {
                artist
            } else {
                format!(r#"{{"artists":[{artist}]}}"#)
            };
            let mut transport = Canned {
                answers: vec![Ok(response), Ok(NO_ALBUM.into())],
                asked: Vec::new(),
            };
            run_with(&args(&dir, &["--full"]), &mut transport, &NO_WAIT).unwrap();
            assert_eq!(transport.asked.len(), 2);
            let saved = sources::load_all(&path).unwrap().unwrap();
            let record = saved.get(&entity, sources::MUSICBRAINZ).unwrap();
            assert_eq!(record.source_id.as_deref(), Some(survivor));
            let Facts::Artist(facts) = &record.facts else {
                panic!("artist facts")
            };
            assert_eq!(
                facts.discography_fetched_at,
                preserve.then_some(17),
                "{case}, empty={empty}"
            );
            assert_eq!(
                facts.discography,
                if preserve { known } else { Vec::new() },
                "{case}, empty={empty}"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn recording_credit_requests_honour_artist_and_album_names() {
    for name in ["Jimi Hendrix", "Electric Ladyland"] {
        let dir = sandbox(&text::normalize(name).replace(' ', "_"));
        let mut tags = RawTags::default();
        for (field, value) in [
            ("artist", "Jimi Hendrix"),
            ("albumartist", "Jimi Hendrix"),
            ("album", "Electric Ladyland"),
            ("title", "All Along the Watchtower"),
            ("musicbrainz_recordingid", "recording-id"),
        ] {
            tags.insert(field, value);
        }
        let catalog = build(
            vec![ScannedFile {
                path: "/music/Hendrix/01.flac".into(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            vec!["/music".into()],
            1,
            &[],
        );
        aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
        let mut transport = Canned {
            answers: Vec::new(),
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits", name]), &mut transport, &NO_WAIT).expect("pass");
        assert_eq!(transport.asked.len(), 1, "recording request for {name}");
        assert!(transport.asked[0].contains("/recording/recording-id?"));
        assert!(
            !sources::sources_path(&dir).exists(),
            "the refused request stored no answer"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The album in the reference library, as a release-group search answers.
const ONE_ALBUM: &str = r#"{"release-groups":[
    {"id":"55555555-5555-4555-8555-555555555555","score":100,"title":"Kind of Blue",
     "primary-type":"Album","first-release-date":"1959-08-17"}]}"#;

/// A search that found nothing — for the tests whose subject is the artist
/// half, where the album still has to be answered because the run asks.
const NO_ALBUM: &str = r#"{"release-groups":[]}"#;

const ONE_ARTIST: &str = r#"{"artists":[
    {"id":"561d854a-6a28-4aa7-8c99-323e6ce46c2a","score":100,"name":"Miles Davis","type":"Person",
     "area":{"name":"United States"},
     "life-span":{"begin":"1926-05-26","end":"1991-09-28","ended":true}}]}"#;

#[test]
fn an_invalid_artist_search_identifier_cannot_replace_a_reviewed_claim() {
    let dir = sandbox("invalid_artist_search_identity");
    let catalog = super::super::load(&args(&dir, &[])).unwrap();
    let artist = &catalog.artists[0];
    assert!(artist.mbid.is_none());
    let entity = EntityRef::of(&catalog, EntityKind::Artist, artist.id).unwrap();
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("11111111-1111-4111-8111-111111111111".into()),
        fetched_at: 1,
        confidence: sources::Confidence::Matched(95),
        facts: Facts::Artist(sources::ArtistFacts {
            wikidata: Some("https://www.wikidata.org/wiki/Q123".into()),
            ..Default::default()
        }),
    });
    let review = held.review_items(&catalog)[0].id.clone();
    held.decide(&catalog, &review, sources::ReviewDecision::Accepted, 2)
        .unwrap();
    let path = sources::sources_path(&dir);
    sources::save(&held, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut transport = Canned {
        answers: vec![
            Ok(format!(
                r#"{{"artists":[{{"id":"not-a-musicbrainz-id","name":"{}","score":100}}]}}"#,
                artist.name
            )),
            Ok(NO_ALBUM.into()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--full"]), &mut transport, &NO_WAIT).unwrap();
    assert!(
        std::fs::read(&path).unwrap() == before,
        "an unreadable candidate must not replace the claim or erase consent"
    );
    assert_eq!(sources::load_all(&path).unwrap().unwrap(), held);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_answer_is_stored_attributed_and_never_as_a_certainty() {
    let dir = sandbox("stored");
    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut transport).expect("a run");

    // One run, both halves: the artist, then the album. They are the same
    // question asked of the same service, and a library gets no report of what
    // its tags say until the albums have been asked about.
    assert_eq!(transport.asked.len(), 2, "asked: {:?}", transport.asked);
    // The name went into the query encoded, not raw.
    assert!(
        transport.asked[0].contains("query=Miles%20Davis"),
        "asked: {}",
        transport.asked[0]
    );
    assert!(
        transport.asked[1].contains("/release-group/"),
        "and the album followed: {}",
        transport.asked[1]
    );

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 2, "one artist, one album");
    let record = held
        .records
        .iter()
        .find(|r| matches!(r.facts, Facts::Artist(_)))
        .expect("the artist row");
    assert_eq!(record.source, sources::MUSICBRAINZ);
    assert_eq!(
        record.source_id.as_deref(),
        Some("561d854a-6a28-4aa7-8c99-323e6ce46c2a")
    );
    assert!(
        !record.confidence.is_certain(),
        "a search never produces a certainty: {:?}",
        record.confidence
    );
    match &record.facts {
        Facts::Artist(a) => {
            assert_eq!(a.area.as_deref(), Some("United States"));
            assert_eq!(a.ended.as_deref(), Some("1991-09-28"));
        }
        other => panic!("expected artist facts, got {other:?}"),
    }

    // And the album row, which is the one with something to compare: the
    // catalog's date comes from the tags, `first_released` from MusicBrainz,
    // and a reissue is where they part company.
    let album = held
        .records
        .iter()
        .find(|r| matches!(r.facts, Facts::Release(_)))
        .expect("the album row");
    match &album.facts {
        Facts::Release(r) => {
            assert_eq!(r.primary_type.as_deref(), Some("Album"));
            assert_eq!(r.first_released.as_deref(), Some("1959-08-17"));
        }
        other => panic!("expected release facts, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_identifier_in_the_tags_is_used_instead_of_a_search() {
    // A library tagged with Picard already carries the MBID. Searching by
    // name there swaps a certainty for a guess, and asks a poorer question
    // besides: a search result is abbreviated, a lookup returns the
    // entity — which is why `ended` was missing and the confidence read
    // 95% for an artist nobody was unsure about.
    let dir = sandbox("bymbid");
    let mut tags = RawTags::default();
    tags.insert("artist", "Marilyn Manson");
    tags.insert("albumartist", "Marilyn Manson");
    tags.insert("album", "Antichrist Superstar");
    tags.insert("title", "The Beautiful People");
    tags.insert(
        "musicbrainz_artistid",
        "c98ff0e1-a92a-4a24-8f21-1a6b1e0b7c0f",
    );
    let catalog = aede_core::model::builder::build(
        vec![ScannedFile {
            path: "/music/Manson/Antichrist/01.flac".to_string(),
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
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(r#"{"id":"c98ff0e1-a92a-4a24-8f21-1a6b1e0b7c0f",
            "name":"Marilyn Manson","type":"Group",
            "disambiguation":"US industrial metal band",
            "area":{"name":"United States"},
            "life-span":{"begin":"1989","ended":false}}"#
            .to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert!(
        transport.asked[0].contains("/artist/c98ff0e1"),
        "it looked the identifier up rather than searching: {}",
        transport.asked[0]
    );
    assert!(!transport.asked[0].contains("query="), "not a search");

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    let record = &held.records[0];
    assert!(
        record.confidence.is_certain(),
        "asked by identifier, answered about that identifier: {:?}",
        record.confidence
    );
    match &record.facts {
        Facts::Artist(a) => {
            assert_eq!(a.active, Some(true), "the boolean a search leaves out");
            assert_eq!(
                a.disambiguation.as_deref(),
                Some("US industrial metal band")
            );
        }
        other => panic!("expected artist facts, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_ambiguous_answer_stores_nothing_at_all() {
    // The refusal has to reach the store, not only the screen: filing one
    // of two equally good answers is the mistake this design exists to
    // prevent, and a test that only read the output would not see it.
    let dir = sandbox("ambiguous");
    let mut transport = Canned {
        answers: vec![
            Ok(r#"{"artists":[
            {"id":"11111111-1111-4111-8111-111111111111","score":90,"name":"Someone"},
            {"id":"22222222-2222-4222-8222-222222222222","score":90,"name":"Someone Else"}]}"#
                .to_string()),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut transport).expect("a run");

    let held = sources::load(&sources::sources_path(&dir)).expect("readable");
    assert!(
        held.is_none_or(|h| h.records.is_empty()),
        "nothing was guessed"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// No waiting in tests: the behaviour under test is the retry, not the sleep.
const NO_WAIT: [std::time::Duration; 3] = [std::time::Duration::ZERO; 3];

#[cfg(feature = "fetch")]
#[test]
fn http_429_is_preserved_as_a_stop_signal() {
    assert_eq!(
        http_refusal(aede_core::http::Error::Status(429)),
        Refusal::TooManyRequests
    );
}

#[test]
fn http_429_is_not_retried_as_a_transient_503() {
    let mut calls = 0;
    let answer: Result<(), Refusal> = request_with_backoff(
        || {
            calls += 1;
            Err(Refusal::TooManyRequests)
        },
        &NO_WAIT,
    );
    assert_eq!(answer, Err(Refusal::TooManyRequests));
    assert_eq!(calls, 1);
}

#[test]
fn a_hiccup_is_waited_out_rather_than_ending_the_run() {
    // What the first real run got wrong: request 5 of 402 came back 503,
    // the whole run stopped, and the message blamed a rate limit that had
    // not been exceeded. A transient refusal lets the next attempt
    // through, which is exactly what tells it apart from a ban.
    let dir = sandbox("hiccup");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::RateLimited),
            Ok(ONE_ARTIST.to_string()),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert_eq!(
        transport.asked.len(),
        3,
        "the refused request, the one that worked, then the album"
    );
    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 1, "and the answer was stored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_timeout_is_retried_once_before_being_reported() {
    // The one failure `ask_with_backoff` does not retry on its own: the
    // service was never reached at all. A single one of those is what a
    // MusicBrainz search occasionally does under load, and reporting it
    // immediately would describe a working service as broken.
    let dir = sandbox("timeout-once");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::Unreachable("timeout: global".to_string())),
            Ok(ONE_ARTIST.to_string()),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert_eq!(
        transport.asked.len(),
        3,
        "the request that timed out, asked again, then the album"
    );
    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 1, "the retry is what got stored");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_timeout_on_the_same_artist_is_reported() {
    // One retry, not a second: a service that could not be reached twice in
    // a row is not a hiccup any more, and asking a third time would only
    // delay the same report.
    let dir = sandbox("timeout-twice");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::Unreachable("timeout: global".to_string())),
            Err(Refusal::Unreachable("timeout: global".to_string())),
            Ok(NO_ALBUM.to_string()),
        ],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect("a run");

    assert_eq!(
        transport.asked.len(),
        3,
        "one try, one retry, then the album — the artist gave up after two"
    );
    let held = sources::load(&sources::sources_path(&dir)).expect("readable");
    assert!(
        held.is_none_or(|h| h.records.is_empty()),
        "nothing was stored for an artist never reached"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn being_told_to_slow_down_stops_the_run_and_keeps_what_was_stored() {
    // The one failure that must not be retried: MusicBrainz answers 503 to
    // *everything* once the rate is exceeded, so carrying on would turn a
    // slow run into a broken one for every program on this address.
    let dir = sandbox("ratelimit");
    let mut transport = Canned {
        answers: vec![
            Err(Refusal::RateLimited),
            Err(Refusal::RateLimited),
            Err(Refusal::RateLimited),
            Err(Refusal::RateLimited),
        ],
        asked: Vec::new(),
    };
    let refused = run_with(&args(&dir, &[]), &mut transport, &NO_WAIT).expect_err("it stops");
    assert_eq!(transport.asked.len(), 4, "it tried, then gave up");
    assert!(
        refused.to_string().contains("4 times in a row"),
        "and says it is a limit rather than a hiccup: {refused}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_second_run_asks_again_only_when_told_to() {
    let dir = sandbox("again");
    let mut first = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut first).expect("a run");

    // Nothing to ask: a second pass over a library costs what changed.
    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(&dir, &[]), &mut second).expect("a run");
    assert!(second.asked.is_empty(), "asked: {:?}", second.asked);

    // Unless asked again explicitly.
    let mut third = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &["--full"]), &mut third).expect("a run");
    assert_eq!(
        third.asked.len(),
        2,
        "both halves again, not just the artist"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_full_artist_refresh_keeps_the_browsed_discography() {
    // `fetch --discography` adds its result to the MusicBrainz artist row.
    // The ordinary artist endpoint never returns that list, so a later
    // `fetch --full` must merge the fresh facts into the row rather than
    // replacing the separately browsed discography with an empty vector.
    let dir = sandbox("keep_discography");
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).expect("an artist");
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("561d854a-6a28-4aa7-8c99-323e6ce46c2a".to_string()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(sources::ArtistFacts {
            discography: vec![sources::KnownRelease {
                mbid: "missing-group".to_string(),
                title: "The Missing Album".to_string(),
                first_released: Some("1970".to_string()),
                primary_type: Some("Album".to_string()),
                secondary_types: Vec::new(),
            }],
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &["--full"]), &mut transport).expect("a full refresh");

    let after = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    let record = after
        .get(&entity, sources::MUSICBRAINZ)
        .expect("the refreshed artist");
    let Facts::Artist(facts) = &record.facts else {
        panic!("expected artist facts")
    };
    assert_eq!(facts.discography.len(), 1);
    assert_eq!(facts.discography[0].title, "The Missing Album");
    assert_eq!(facts.area.as_deref(), Some("United States"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dry_run_asks_nothing() {
    let dir = sandbox("dryrun");
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(&dir, &["--dry-run"]), &mut transport).expect("a run");
    assert!(transport.asked.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn what_you_corrected_by_hand_survives_a_fetch() {
    // The guarantee that makes hand-correction usable at all: records are
    // keyed on (entity, source), so a value filed under `manual` is not
    // something MusicBrainz can overwrite. Without it, correcting a wrong
    // answer would last until the next run and no further.
    let dir = sandbox("manual");
    let mut held = sources::Sources::default();
    let catalog = super::super::load(&args(&dir, &[])).expect("a catalog");
    let entity = EntityRef::of(&catalog, EntityKind::Artist, 0).expect("an artist");
    held.set(SourceRecord {
        key: entity.key.clone(),
        source: "manual".to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: aede_core::sources::Confidence::Identified,
        facts: Facts::Artist(aede_core::sources::ArtistFacts {
            area: Some("Alton, Illinois".to_string()),
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(NO_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run(&args(&dir, &["--full"]), &mut transport).expect("a run");

    let after = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(after.records.len(), 2, "two sources, two rows");
    let mine = after
        .get(&entity, "manual")
        .expect("what I wrote is still there");
    match &mine.facts {
        Facts::Artist(a) => assert_eq!(a.area.as_deref(), Some("Alton, Illinois")),
        other => panic!("expected artist facts, got {other:?}"),
    }
    assert!(
        after.get(&entity, sources::MUSICBRAINZ).is_some(),
        "and what was fetched sits beside it rather than on top"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_build_that_cannot_say_who_it_is_refuses_to_ask() {
    // What the first real run cost: an empty contact went out as
    // an empty contact address, MusicBrainz throttled it as anonymous, and the
    // 503 pointed at the rate limit rather than at the manifest.
    assert!(identity("0.3.0", "").is_err());
    assert!(identity("0.3.0", "   ").is_err());
    let refused = identity("0.3.0", "").expect_err("refused");
    assert!(
        refused.to_string().contains("repository.workspace"),
        "and names the fix: {refused}"
    );
    assert_eq!(
        identity("0.3.0", "https://example.org/aede").expect("a header"),
        "aede/0.3.0 ( https://example.org/aede )"
    );
}

#[test]
fn the_estimate_is_in_the_unit_the_formatter_expects() {
    // 403 artists printed "about 0 s": the count was multiplied into
    // seconds and handed to a formatter that takes milliseconds. The kind
    // of mistake that survives review and dies to one assertion.
    let ms = 403_u64 * aede_core::musicbrainz::REQUEST_INTERVAL.as_millis() as u64;
    let shown = ui::long_duration(ms);
    assert!(
        shown.contains("min"),
        "403 requests is minutes, not {shown}"
    );
}

#[test]
fn a_name_becomes_a_query_value_and_not_a_second_parameter() {
    // The one that matters: a `&` in a band's name would otherwise end the
    // query and start a parameter of its own, and the search would quietly
    // be about something else.
    assert_eq!(encode("Miles Davis"), "Miles%20Davis");
    assert_eq!(encode("Simon & Garfunkel"), "Simon%20%26%20Garfunkel");
    assert_eq!(encode("AC/DC"), "AC%2FDC");
    assert_eq!(encode("Björk"), "Bj%C3%B6rk", "UTF-8, byte by byte");
    assert_eq!(
        encode("a-b_c.d~e"),
        "a-b_c.d~e",
        "the unreserved set is kept"
    );
}

#[test]
fn two_passes_asked_for_together_both_run() {
    // The defect this pins: the passes were three `return`s in a row, so
    // `--covers --discography` ran the covers and dropped the discography
    // without a word. An option that cannot be honoured is refused everywhere
    // else in this program; this one could be honoured and was not.
    let dir = sandbox("two_passes");

    // First an ordinary fetch, so that both passes have something identified
    // to work from: a discography needs the artist's identifier, a cover needs
    // the album's.
    let mut first = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut first, &NO_WAIT).expect("the ordinary fetch");
    accept_matches(&dir);

    let mut both = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--covers", "--discography"]),
        &mut both,
        &NO_WAIT,
    )
    .expect("both passes run");

    // Each pass asks its own service, so what was asked says which ran. With
    // the old three-`return` dispatch only the second of these appeared.
    assert!(
        both.asked
            .iter()
            .any(|url| url.contains("release-group?artist=")),
        "the discography pass was skipped: {:?}",
        both.asked
    );
    assert!(
        both.asked
            .iter()
            .any(|url| url.contains("coverartarchive.org")),
        "the cover pass was skipped: {:?}",
        both.asked
    );
    // And in the order the passes are declared in, not the order typed.
    let discography = both
        .asked
        .iter()
        .position(|url| url.contains("release-group?artist="))
        .expect("asked");
    let covers = both
        .asked
        .iter()
        .position(|url| url.contains("coverartarchive.org"))
        .expect("asked");
    assert!(discography < covers, "{:?}", both.asked);
}

#[test]
fn the_passes_run_in_their_own_order_and_not_the_typed_one() {
    // Fixed rather than typed, because they go out from the artist: who they
    // are, what they recorded, what the records look like. Asking about the
    // albums first would be asking before the fetch that names them.
    let typed_one_way = Pass::asked_for(&args(
        std::path::Path::new("/tmp"),
        &["--covers", "--summaries", "--discography"],
    ));
    let typed_another = Pass::asked_for(&args(
        std::path::Path::new("/tmp"),
        &["--discography", "--covers", "--summaries"],
    ));
    assert_eq!(
        typed_one_way,
        vec![Pass::Summaries, Pass::Discography, Pass::Covers]
    );
    assert_eq!(
        typed_one_way, typed_another,
        "two orders of the same three options are one run"
    );

    assert_eq!(
        Pass::asked_for(&args(
            std::path::Path::new("/tmp"),
            &["--fanart", "--logos", "--banners"],
        )),
        vec![Pass::Logos],
        "the broad and legacy flags share one Fanart.tv metadata pass"
    );

    assert!(Pass::asked_for(&args(std::path::Path::new("/tmp"), &[])).is_empty());

    // And only the summaries pass can work without a catalog, which is what
    // decides whether one is loaded at all.
    assert!(!Pass::Summaries.needs_the_catalog());
    assert!(Pass::Discography.needs_the_catalog());
    assert!(Pass::Covers.needs_the_catalog());
}

#[test]
fn label_identification_precedes_its_wikipedia_summary() {
    let passes = Pass::asked_for(&args(
        std::path::Path::new("/tmp"),
        &["--summaries", "--labels"],
    ));
    assert_eq!(passes, vec![Pass::Labels, Pass::Summaries]);
}

#[test]
fn an_unusable_size_stops_the_run_before_any_pass_asks_anything() {
    // `--size` is read once for the whole run now rather than inside the
    // cover pass, and a value the archive does not generate must still be
    // refused before a summaries pass has spent ten minutes on the network.
    let dir = sandbox("bad_size");
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    let error = run_with(
        &args(&dir, &["--summaries", "--covers", "--size=800"]),
        &mut transport,
        &NO_WAIT,
    )
    .expect_err("800 is not a width the archive generates");
    assert!(error.to_string().contains("--size takes"), "{error}");
    assert!(
        transport.asked.is_empty(),
        "and nothing was asked before it was refused"
    );
}

#[test]
fn a_name_given_to_a_second_pass_narrows_it_instead_of_being_swallowed() {
    // The defect: `aede fetch --discography mika` ran over the whole library
    // and said nothing about the word it had ignored — an argument that could
    // not be honoured, dropped rather than refused, which is the one thing
    // this program is most careful about elsewhere.
    let dir = sandbox("named_pass");
    let mut first = Canned {
        answers: vec![Ok(ONE_ARTIST.to_string()), Ok(ONE_ALBUM.to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &[]), &mut first, &NO_WAIT).expect("the ordinary fetch");
    accept_matches(&dir);

    // A name nobody here answers to: nothing is asked at all.
    let mut nobody = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--discography", "mika"]),
        &mut nobody,
        &NO_WAIT,
    )
    .expect("the pass ran");
    assert!(
        nobody.asked.is_empty(),
        "a name reaching nobody must not browse the whole shelf: {:?}",
        nobody.asked
    );

    // And the name of somebody who is here does reach them.
    let mut somebody = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--discography", "miles"]),
        &mut somebody,
        &NO_WAIT,
    )
    .expect("the pass ran");
    assert!(
        somebody
            .asked
            .iter()
            .any(|url| url.contains("release-group?artist=")),
        "the name matched and the pass ran: {:?}",
        somebody.asked
    );
}

#[test]
fn a_name_reaches_an_album_by_its_title_or_by_its_artist() {
    // The rule the ordinary fetch already used, now shared by every pass:
    // `--covers manson` should find the records as well as the person.
    assert!(reaches(&[], &["anything"]), "no name means the whole shelf");
    assert!(reaches(&["miles".to_string()], &["Miles Davis", ""]));
    assert!(reaches(&["blue".to_string()], &["", "Kind of Blue"]));
    assert!(!reaches(
        &["mika".to_string()],
        &["Miles Davis", "Kind of Blue"]
    ));
    // Case and accents go through `normalize`, as everywhere else.
    assert!(reaches(&[text::normalize("MILES")], &["Miles Davis"]));
}

#[test]
fn nothing_matched_says_which_of_the_two_nothings_it_is() {
    // "No such artist here" and "that artist is already done" are different
    // problems with different next steps, and the general "run fetch first"
    // sends the second of them to re-run a pass with nothing to do.
    let names = vec!["mika".to_string()];
    assert!(nothing_named(&names, &EVERYTHING, 0).contains("nothing here matches mika"));
    let done = nothing_named(&names, &EVERYTHING, 3);
    assert!(done.contains("3 artists"), "{done}");
    assert!(done.contains("--full"), "{done}");
}

#[test]
fn a_lookup_asks_for_the_memberships_and_stores_them_dated() {
    // **An include nobody asks for is a field nobody has.** The parser can read
    // a line-up perfectly and still show nothing, for ever, because the request
    // never asked for the relations — which is a failure with no error message
    // anywhere. So the URL is asserted as well as the result.
    //
    // The answer is the one MusicBrainz really gave for Judas Priest, cut to
    // the fields this reads.
    let dir = sandbox("members");
    let mut tags = RawTags::default();
    tags.insert("artist", "Judas Priest");
    tags.insert("albumartist", "Judas Priest");
    tags.insert("album", "British Steel");
    tags.insert("title", "Breaking the Law");
    tags.insert(
        "musicbrainz_artistid",
        "6b335658-22c8-485d-93de-0bc29a1d0349",
    );
    let catalog = aede_core::model::builder::build(
        vec![ScannedFile {
            path: "/music/Priest/British Steel/01.flac".to_string(),
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
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");

    let mut transport = Canned {
        answers: vec![Ok(r#"{"id":"6b335658-22c8-485d-93de-0bc29a1d0349",
            "name":"Judas Priest","type":"Group",
            "life-span":{"begin":"1969","ended":false},
            "relations":[
              {"type":"member of band","direction":"backward","target-type":"artist",
               "begin":"1969","end":"1970","ended":true,
               "attributes":["guitar family","original"],
               "artist":{"id":"3651334d-6513-40e1-adbb-423acf0ad3d6",
                         "name":"Ernie Chataway","type":"Person"}},
              {"type":"instrumental supporting musician","direction":"backward",
               "target-type":"artist","begin":"2018","end":null,"ended":false,
               "attributes":["electric guitar"],
               "artist":{"id":"2b9aed4d-769e-4c38-96e4-586ac59ce668",
                         "name":"Andy Sneap","type":"Person"}}]}"#
            .to_string())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--artists"]), &mut transport, &NO_WAIT).expect("a run");

    assert!(
        transport.asked[0].contains("artist-rels"),
        "the request asked for the relations: {}",
        transport.asked[0]
    );

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    let Facts::Artist(facts) = &held.records[0].facts else {
        panic!("expected artist facts");
    };
    assert_eq!(facts.line_up().count(), 2, "both relations name a player");
    assert!(facts.bands().next().is_none(), "and neither names a band");

    // The whole reason the dates are worth asking for: the band as it stood in
    // a given year. Ernie Chataway had left by 1980; Andy Sneap had not
    // arrived, so 1980 holds neither.
    let (in_1969, _) = facts.line_up_in(1969);
    assert_eq!(
        in_1969.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["Ernie Chataway"]
    );
    let (in_2020, _) = facts.line_up_in(2020);
    assert_eq!(
        in_2020.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        vec!["Andy Sneap"],
        "\"end\": null beside \"ended\": false is a musician still playing"
    );
}

#[test]
fn redirected_recording_lookups_keep_trusted_credits_and_explicit_reviews() {
    use aede_core::credit_coverage::{self, CreditStatus};
    let queried = "11111111-1111-4111-8111-111111111111";
    let canonical = "22222222-2222-4222-8222-222222222222";
    for legacy in [false, true] {
        for decision in [
            None,
            Some(sources::ReviewDecision::Accepted),
            Some(sources::ReviewDecision::Rejected),
        ] {
            let dir = sandbox(&format!("redirect_recording_trust_{legacy}_{decision:?}"));
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Example Band"),
                ("albumartist", "Example Band"),
                ("album", "Example Album"),
                ("title", "Example Song"),
                ("musicbrainz_recordingid", queried),
            ] {
                tags.insert(key, value);
            }
            let catalog = build(
                vec![ScannedFile {
                    path: dir.join("music/01.flac").to_string_lossy().into_owned(),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }],
                vec![dir.join("music").to_string_lossy().into_owned()],
                1,
                &[],
            );
            aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
            let entity = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
            let mut held = sources::Sources::default();
            held.set(SourceRecord {
                key: entity.key.clone(),
                source: sources::MUSICBRAINZ.into(),
                source_id: (!legacy).then(|| queried.into()),
                fetched_at: 1,
                confidence: sources::Confidence::Identified,
                facts: Facts::Track(sources::TrackFacts {
                    recording: legacy.then(|| queried.into()),
                    ..Default::default()
                }),
            });
            if let Some(decision) = decision {
                held.set_review(sources::SourceReview {
                    entity: entity.clone(),
                    source: sources::MUSICBRAINZ.into(),
                    source_id: (!legacy).then(|| queried.into()),
                    decision,
                    reviewed_at: 17,
                });
            }
            let path = sources::sources_path(&dir);
            sources::save(&held, &path).unwrap();
            let response = |id| {
                format!(
                    r#"{{"id":"{id}","title":"Example Song","relations":[
            {{"type":"producer","target-type":"artist","artist":{{
                "id":"55555555-5555-4555-8555-555555555555","name":"Example Producer"}}}}
        ]}}"#
                )
            };
            for canonical in [canonical, "33333333-3333-4333-8333-333333333333"] {
                let mut transport = Canned {
                    answers: vec![Ok(response(canonical))],
                    asked: Vec::new(),
                };
                run_with(
                    &args(&dir, &["--credits", "--full"]),
                    &mut transport,
                    &NO_WAIT,
                )
                .unwrap();
                let saved = sources::load_all(&path).unwrap().unwrap();
                let record = saved.get(&entity, sources::MUSICBRAINZ).unwrap();
                assert_eq!(record.source_id.as_deref(), Some(canonical));
                let allowed = decision != Some(sources::ReviewDecision::Rejected);
                assert_eq!(
                    credit_coverage::recordings(&catalog, &saved)[0].status,
                    if allowed {
                        CreditStatus::Credited
                    } else {
                        CreditStatus::Untrusted
                    },
                    "an exact redirected recording lookup must retain usable credits"
                );
                assert_eq!(
                    saved
                        .credit_links(&catalog)
                        .iter()
                        .filter(|link| link.trusted)
                        .count(),
                    usize::from(allowed)
                );
                let mut retagged = catalog.clone();
                retagged.recordings[0].mbid = Some(canonical.into());
                let reloaded = sources::load_all(&path).unwrap().unwrap();
                assert_eq!(
                    credit_coverage::recordings(&retagged, &reloaded)[0].status,
                    if allowed {
                        CreditStatus::Credited
                    } else {
                        CreditStatus::Untrusted
                    }
                );
                assert_eq!(
                    reloaded
                        .credit_links(&retagged)
                        .iter()
                        .filter(|link| link.trusted)
                        .count(),
                    usize::from(allowed),
                    "a saved recording claim must remain usable after canonical retagging"
                );
                assert_eq!(
                    saved
                        .review_for(record)
                        .map(|review| (review.decision, review.reviewed_at)),
                    decision.map(|decision| (decision, 17))
                );
                if decision.is_none() {
                    let mut different = catalog.clone();
                    different.recordings[0].mbid =
                        Some("44444444-4444-4444-8444-444444444444".into());
                    assert!(
                        saved
                            .credit_links(&different)
                            .iter()
                            .all(|link| !link.trusted)
                    );
                }
                let catalog_path = aede_core::store::catalog_path(&dir);
                aede_core::store::save(&retagged, &catalog_path).unwrap();
                let mut refreshed_canonical = Canned {
                    answers: vec![Ok(response(canonical))],
                    asked: Vec::new(),
                };
                run_with(
                    &args(&dir, &["--credits", "--full"]),
                    &mut refreshed_canonical,
                    &NO_WAIT,
                )
                .unwrap();
                let refreshed = sources::load_all(&path).unwrap().unwrap();
                let record = refreshed.get(&entity, sources::MUSICBRAINZ).unwrap();
                assert_eq!(
                    refreshed
                        .review_for(record)
                        .map(|review| (review.decision, review.reviewed_at)),
                    decision.map(|decision| (decision, 17))
                );
                for current in [&catalog, &retagged] {
                    assert_eq!(
                        credit_coverage::recordings(current, &refreshed)[0].status,
                        if allowed {
                            CreditStatus::Credited
                        } else {
                            CreditStatus::Untrusted
                        }
                    );
                    assert_eq!(
                        refreshed
                            .credit_links(current)
                            .iter()
                            .filter(|link| link.trusted)
                            .count(),
                        usize::from(allowed)
                    );
                }
                aede_core::store::save(&catalog, &catalog_path).unwrap();
                let before = std::fs::read(&path).unwrap();
                let mut malformed = Canned {
                    answers: vec![Ok(response("not-an-mbid"))],
                    asked: Vec::new(),
                };
                run_with(
                    &args(&dir, &["--credits", "--full"]),
                    &mut malformed,
                    &NO_WAIT,
                )
                .unwrap();
                assert_eq!(
                    std::fs::read(&path).unwrap(),
                    before,
                    "an unreadable redirect must preserve the last completed lookup and its review"
                );
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn inconsistent_recording_receipts_remain_retryable_without_breaking_legacy_cache() {
    let queried = "11111111-1111-4111-8111-111111111111";
    let source_id = "22222222-2222-4222-8222-222222222222";
    let conflicting = "33333333-3333-4333-8333-333333333333";
    for (case, local, recording, must_retry) in [
        ("conflicting alias", queried, Some(conflicting), true),
        ("absent alias evidence", queried, None, true),
        ("conflicting canonical", source_id, Some(conflicting), true),
        ("legacy canonical", source_id, None, false),
    ] {
        let dir = sandbox(&format!("inconsistent_recording_alias_receipt_{case}"));
        let mut tags = RawTags::default();
        for (key, value) in [
            ("artist", "Example Artist"),
            ("albumartist", "Example Artist"),
            ("album", "Example Album"),
            ("title", "Example Song"),
            ("musicbrainz_recordingid", local),
        ] {
            tags.insert(key, value);
        }
        let catalog = build(
            vec![ScannedFile {
                path: dir.join("music/01.flac").to_string_lossy().into_owned(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            vec![dir.join("music").to_string_lossy().into_owned()],
            1,
            &[],
        );
        aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
        let entity = EntityRef::of(&catalog, EntityKind::Track, 0).unwrap();
        let mut held = sources::Sources::default();
        held.set(SourceRecord {
            key: entity.key.clone(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some(source_id.into()),
            fetched_at: 1,
            confidence: sources::Confidence::Identified,
            facts: Facts::Track(sources::TrackFacts {
                recording: recording.map(str::to_string),
                relationships_complete: true,
                relationships_for: Some(queried.into()),
                ..Default::default()
            }),
        });
        let path = sources::sources_path(&dir);
        sources::save(&held, &path).unwrap();
        let reloaded = sources::load_all(&path).unwrap().unwrap();
        assert_eq!(
            reloaded
                .get(&entity, sources::MUSICBRAINZ)
                .unwrap()
                .musicbrainz_lookup_alias(),
            None
        );
        if local == queried {
            assert!(!reloaded.is_trusted(
                &catalog,
                reloaded.get(&entity, sources::MUSICBRAINZ).unwrap()
            ));
        }
        let before = std::fs::read(&path).unwrap();
        if !must_retry {
            let mut reused = Canned {
                answers: Vec::new(),
                asked: Vec::new(),
            };
            run_with(&args(&dir, &["--credits"]), &mut reused, &NO_WAIT).unwrap();
            assert!(reused.asked.is_empty());
            assert_eq!(std::fs::read(&path).unwrap(), before);
            std::fs::remove_dir_all(dir).unwrap();
            continue;
        }
        let mut cancelled = Canned {
            answers: vec![Err(Refusal::TooManyRequests)],
            asked: Vec::new(),
        };
        assert!(
            run_with(&args(&dir, &["--credits"]), &mut cancelled, &NO_WAIT).is_err(),
            "an inconsistent receipt cannot conceal the requested lookup"
        );
        assert_eq!(cancelled.asked.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let mut retry = Canned {
            answers: vec![Ok(format!(
                r#"{{"id":"{local}","title":"Example Song","relations":[]}}"#
            ))],
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits"]), &mut retry, &NO_WAIT).unwrap();
        assert_eq!(retry.asked.len(), 1);
        let recovered = sources::load_all(&path).unwrap().unwrap();
        assert!(recovered.is_trusted(
            &catalog,
            recovered.get(&entity, sources::MUSICBRAINZ).unwrap()
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn recording_credit_receipts_follow_the_queried_identity_after_a_local_correction() {
    let dir = sandbox("corrected_recording_credit_identity");
    let path = sources::sources_path(&dir);
    let old = "11111111-1111-4111-8111-111111111111";
    let new = "22222222-2222-4222-8222-222222222222";
    let canonical = "33333333-3333-4333-8333-333333333333";
    let audio_path = dir.join("music/01.flac");
    let make_catalog = |mbid: &str| {
        let mut tags = RawTags::default();
        for (key, value) in [
            ("artist", "Example Player"),
            ("albumartist", "Example Player"),
            ("album", "Example Album"),
            ("title", "Example Performance"),
            ("musicbrainz_recordingid", mbid),
        ] {
            tags.insert(key, value);
        }
        build(
            vec![ScannedFile {
                path: audio_path.to_string_lossy().into_owned(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            vec![dir.join("music").to_string_lossy().into_owned()],
            1,
            &[],
        )
    };
    let catalog_path = aede_core::store::catalog_path(&dir);
    let response =
        |mbid: &str| format!(r#"{{"id":"{mbid}","title":"Example Performance","relations":[]}}"#);
    aede_core::store::save(&make_catalog(old), &catalog_path).unwrap();
    let mut first = Canned {
        answers: vec![Ok(response(old))],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut first, &NO_WAIT).unwrap();
    assert_eq!(first.asked.len(), 1);

    // A corrected recording at the same file path is a new question. Its lookup
    // may itself follow a merged identifier; cache that exact query as well.
    aede_core::store::save(&make_catalog(new), &catalog_path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut interrupted = Canned {
        answers: vec![Err(Refusal::TooManyRequests)],
        asked: Vec::new(),
    };
    assert!(run_with(&args(&dir, &["--credits"]), &mut interrupted, &NO_WAIT).is_err());
    assert_eq!(interrupted.asked.len(), 1);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "an interrupted correction retains its previous receipt and remains retryable"
    );
    let mut changed = Canned {
        answers: vec![Ok(response(canonical))],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut changed, &NO_WAIT).unwrap();
    assert_eq!(
        changed.asked.len(),
        1,
        "a completed lookup for an old recording cannot suppress the corrected recording"
    );
    assert!(changed.asked[0].contains(&format!("/recording/{new}?")));
    let held = sources::load_all(&path).unwrap().unwrap();
    assert_eq!(held.records[0].source_id.as_deref(), Some(canonical));
    assert!(matches!(&held.records[0].facts, Facts::Track(facts)
        if facts.relationships_for.as_deref() == Some(new)));
    let mut repeated = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut repeated, &NO_WAIT).unwrap();
    assert!(
        repeated.asked.is_empty(),
        "a merged lookup is still a completed answer"
    );
    // Retagging the file with the canonical ID already returned by that lookup
    // must reuse the answer too; this is not a different recording.
    aede_core::store::save(&make_catalog(canonical), &catalog_path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut canonicalized = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut canonicalized, &NO_WAIT).unwrap();
    assert!(
        canonicalized.asked.is_empty(),
        "the canonical ID already confirmed by the cached lookup needs no new request"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut refresh = Canned {
        answers: vec![Ok(response(canonical))],
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--credits", "--full"]),
        &mut refresh,
        &NO_WAIT,
    )
    .unwrap();
    let refreshed = sources::load_all(&path).unwrap().unwrap();
    assert_eq!(
        aede_core::credit_coverage::recordings(&make_catalog(new), &refreshed)[0].status,
        aede_core::credit_coverage::CreditStatus::Empty,
        "refreshing the confirmed canonical recording must retain its earlier verified alias"
    );
    aede_core::store::save(&make_catalog(new), &catalog_path).unwrap();
    let mut restored_alias = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut restored_alias, &NO_WAIT).unwrap();
    assert!(
        restored_alias.asked.is_empty(),
        "the confirmed alias remains a completed lookup"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn completed_recording_credit_cache_is_shared_by_all_placements() {
    use aede_core::credit_coverage::{self, CreditStatus};
    const RECORDING: &str = "11111111-1111-4111-8111-111111111111";
    let dir = super::super::canonical(&sandbox("canonical_recording_credit_cache"));
    let audio = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    let files = ["Album A", "Album B"]
        .into_iter()
        .map(|album| {
            let path = dir.join("music").join(album).join("01.flac");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, audio).unwrap();
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", "Local Player"),
                ("albumartist", "Local Player"),
                ("album", album),
                ("title", "Shared Performance"),
                ("musicbrainz_recordingid", RECORDING),
            ] {
                tags.insert(key, value);
            }
            ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: audio.len() as u64,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    let catalog = build(
        files,
        vec![dir.join("music").to_string_lossy().into_owned()],
        1,
        &[],
    );
    assert_eq!(catalog.recordings.len(), 1);
    let catalog_path = aede_core::store::catalog_path(&dir);
    aede_core::store::save(&catalog, &catalog_path).unwrap();
    let catalog_before = std::fs::read(&catalog_path).unwrap();
    let source_path = sources::sources_path(&dir);
    let first_folder = dir.join("music/Album A");
    let second_folder = dir.join("music/Album B");
    let response = format!(r#"{{"id":"{RECORDING}","title":"Shared Performance","relations":[]}}"#);
    let mut initial = Canned {
        answers: vec![Ok(response.clone())],
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--credits", second_folder.to_str().unwrap()]),
        &mut initial,
        &NO_WAIT,
    )
    .unwrap();
    assert_eq!(initial.asked.len(), 1);
    let cached = sources::load_all(&source_path).unwrap().unwrap();
    assert_eq!(cached.records.len(), 1);
    assert_eq!(
        cached.records[0].key,
        second_folder.join("01.flac").to_string_lossy()
    );
    assert_eq!(
        credit_coverage::recordings(&catalog, &cached)[0].status,
        CreditStatus::Empty
    );

    let source_before = std::fs::read(&source_path).unwrap();
    let mut repeated = Vec::new();
    for (label, folder) in [
        ("global", None),
        ("other placement", Some(first_folder.as_path())),
        ("original placement", Some(second_folder.as_path())),
    ] {
        let mut options = vec!["--credits"];
        if let Some(folder) = folder {
            options.push(folder.to_str().unwrap());
        }
        let mut transport = Canned {
            answers: Vec::new(),
            asked: Vec::new(),
        };
        run_with(&args(&dir, &options), &mut transport, &NO_WAIT).unwrap();
        repeated.push((label, transport.asked.len()));
        assert_eq!(std::fs::read(&source_path).unwrap(), source_before);
    }
    let mut full = Canned {
        answers: vec![Ok(response)],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits", "--full"]), &mut full, &NO_WAIT).unwrap();
    assert_eq!(
        full.asked.len(),
        1,
        "full deliberately repeats a completed answer"
    );

    // A completed answer remains cached while pending or rejected. Reusing
    // that receipt does not make its credits eligible for trusted queries.
    let mut controls = Vec::new();
    for (label, complete, confidence, source_id, decision, status, expected) in [
        (
            "absent",
            false,
            sources::Confidence::Identified,
            RECORDING,
            None,
            CreditStatus::Waiting,
            1,
        ),
        (
            "incomplete",
            false,
            sources::Confidence::Identified,
            RECORDING,
            None,
            CreditStatus::Waiting,
            1,
        ),
        (
            "pending",
            true,
            sources::Confidence::matched(42),
            RECORDING,
            None,
            CreditStatus::Untrusted,
            0,
        ),
        (
            "accepted",
            true,
            sources::Confidence::matched(42),
            RECORDING,
            Some(sources::ReviewDecision::Accepted),
            CreditStatus::Empty,
            0,
        ),
        (
            "rejected",
            true,
            sources::Confidence::Identified,
            RECORDING,
            Some(sources::ReviewDecision::Rejected),
            CreditStatus::Untrusted,
            0,
        ),
    ] {
        let mut held = if label == "absent" {
            sources::Sources::default()
        } else {
            cached.clone()
        };
        if let Some(record) = held.records.first_mut() {
            record.confidence = confidence;
            record.source_id = Some(source_id.into());
            let Facts::Track(facts) = &mut record.facts else {
                panic!("recording facts");
            };
            facts.relationships_complete = complete;
        }
        if let Some(decision) = decision {
            let record = &held.records[0];
            held.set_review(sources::SourceReview {
                entity: record.entity(),
                source: record.source.clone(),
                source_id: record.source_id.clone(),
                decision,
                reviewed_at: 2,
            });
        }
        assert_eq!(
            credit_coverage::recordings(&catalog, &held)[0].status,
            status,
            "{label}"
        );
        sources::save(&held, &source_path).unwrap();
        let before = std::fs::read(&source_path).unwrap();
        let mut transport = Canned {
            answers: Vec::new(),
            asked: Vec::new(),
        };
        run_with(
            &args(&dir, &["--credits", first_folder.to_str().unwrap()]),
            &mut transport,
            &NO_WAIT,
        )
        .unwrap();
        controls.push((label, transport.asked.len(), expected));
        assert_eq!(std::fs::read(&source_path).unwrap(), before);
    }
    assert_eq!(std::fs::read(&catalog_path).unwrap(), catalog_before);
    for folder in [&first_folder, &second_folder] {
        assert_eq!(std::fs::read(folder.join("01.flac")).unwrap(), audio);
    }
    assert!(!dir.join("user.json").exists());
    assert!(!dir.join("conclusions.json").exists());
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(
        repeated,
        [
            ("global", 0),
            ("other placement", 0),
            ("original placement", 0)
        ],
        "a canonical completed lookup is not repeated through another placement"
    );
    assert!(
        controls
            .iter()
            .all(|(_, actual, expected)| actual == expected),
        "completed-answer controls: {controls:?}"
    );
}

#[test]
fn credits_fetches_recording_and_work_relationships_in_one_request() {
    let dir = sandbox("rich_credits");
    let mut tags = RawTags::default();
    tags.insert("artist", "Jimi Hendrix");
    tags.insert("albumartist", "Jimi Hendrix");
    tags.insert("album", "Electric Ladyland");
    tags.insert("title", "All Along the Watchtower");
    tags.insert("musicbrainz_recordingid", "recording-id");
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Hendrix/01.flac".to_string(),
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
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");
    let mut transport = Canned {
        answers: vec![Ok(r#"{
          "id":"recording-id","title":"All Along the Watchtower",
          "relations":[
            {"id":"performance","type":"instrument","type-id":"instrument-type",
             "target-type":"artist","target-credit":"Jimi Hendrix",
             "attributes":["guitar"],
             "artist":{"id":"hendrix-id","name":"Jimi Hendrix"}},
            {"id":"work-link","type":"performance","target-type":"work",
             "work":{"id":"work-id","title":"All Along the Watchtower",
               "relations":[
                 {"id":"composer","type":"composer","type-id":"composer-type",
                  "target-type":"artist","target-credit":"Robert Dylan",
                  "attributes":[],
                  "artist":{"id":"dylan-id","name":"Bob Dylan"}},
                 {"id":"part-rel","type":"parts",
                  "type-id":"ca8d3642-ce5f-49f8-91f2-125d72524e6a",
                  "direction":"backward","target-type":"work",
                  "attributes":["movement"],"ordering-key":1,
                  "work":{"id":"parent-id","title":"A Complete Work"}}
               ]}}
          ]}"#
        .to_string())],
        asked: Vec::new(),
    };

    run_with(&args(&dir, &["--credits"]), &mut transport, &NO_WAIT).expect("fetch credits");
    assert_eq!(transport.asked.len(), 1);
    assert!(transport.asked[0].contains("artist-rels+work-rels+work-level-rels"));

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("stored");
    let Facts::Track(facts) = &held.records[0].facts else {
        panic!("track facts");
    };
    assert!(facts.relationships_complete);
    assert_eq!(facts.credits[0].role, "instrument");
    assert_eq!(facts.works[0].credits[0].role, "composer");
    assert_eq!(facts.works[0].parents[0].mbid, "parent-id");
    assert_eq!(facts.works[0].parents[0].order, Some(1));

    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut second, &NO_WAIT).expect("already complete");
    assert!(
        second.asked.is_empty(),
        "a complete relationship lookup is not repeated"
    );
}

#[test]
fn merged_edition_lookups_keep_the_queried_scope_and_complete_only_that_edition() {
    use aede_core::credit_coverage::{self, CreditStatus};
    let queried = "11111111-1111-4111-8111-111111111111";
    let surviving = "22222222-2222-4222-8222-222222222222";
    let group = "33333333-3333-4333-8333-333333333333";
    let different = "44444444-4444-4444-8444-444444444444";
    let producer = "55555555-5555-4555-8555-555555555555";
    for grouped in [false, true] {
        for primary in [false, true] {
            for decision in [
                None,
                Some(sources::ReviewDecision::Accepted),
                Some(sources::ReviewDecision::Rejected),
            ] {
                let dir = sandbox(&format!(
                    "merged_edition_credit_scope_{grouped}_{primary}_{decision:?}"
                ));
                let make_catalog = |edition| {
                    let mut tags = RawTags::default();
                    for (key, value) in [
                        ("artist", "Example Band"),
                        ("albumartist", "Example Band"),
                        ("album", "Example Album"),
                        ("title", "Example Song"),
                        ("musicbrainz_albumid", edition),
                    ] {
                        tags.insert(key, value);
                    }
                    if grouped {
                        tags.insert("musicbrainz_releasegroupid", group);
                    }
                    build(
                        vec![ScannedFile {
                            path: dir.join("music/01.flac").to_string_lossy().into_owned(),
                            size: 1,
                            mtime: 1,
                            tags,
                            folder_cover: None,
                            sidecar: None,
                            integrity: None,
                            fingerprint: None,
                        }],
                        vec![dir.join("music").to_string_lossy().into_owned()],
                        1,
                        &[],
                    )
                };
                let catalog = make_catalog(queried);
                aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
                let path = sources::sources_path(&dir);
                let entity = EntityRef::of(&catalog, EntityKind::Release, 0).unwrap();
                if let Some(decision) = decision {
                    let mut held = sources::Sources::default();
                    let source_id = if grouped { group } else { queried };
                    held.set(SourceRecord {
                        key: entity.key.clone(),
                        source: sources::MUSICBRAINZ.into(),
                        source_id: Some(source_id.into()),
                        fetched_at: 1,
                        confidence: sources::Confidence::Identified,
                        facts: Facts::Release(sources::ReleaseFacts {
                            edition_mbid: Some(queried.into()),
                            ..Default::default()
                        }),
                    });
                    held.set_review(sources::SourceReview {
                        entity: entity.clone(),
                        source: sources::MUSICBRAINZ.into(),
                        source_id: Some(source_id.into()),
                        decision,
                        reviewed_at: 17,
                    });
                    sources::save(&held, &path).unwrap();
                }
                let grouped_facts = if grouped {
                    format!(r#", "release-group":{{"id":"{group}","title":"Example Album"}}"#)
                } else {
                    String::new()
                };
                let mut transport = Canned {
                    answers: vec![Ok(format!(
                        r#"{{"id":"{surviving}","title":"Example Album"{grouped_facts},
              "relations":[{{"type":"producer","target-type":"artist",
                "artist":{{"id":"{producer}","name":"Example Producer"}}}}]}}"#
                    ))],
                    asked: Vec::new(),
                };
                if primary {
                    let mut held = sources::load_all(&path).unwrap().unwrap_or_default();
                    let targets =
                        super::super::releases::targets(&catalog, &held, &[], &EVERYTHING, true);
                    super::super::releases::run(
                        &mut transport,
                        &NO_WAIT,
                        &targets,
                        &mut held,
                        &path,
                        0,
                        1,
                    )
                    .unwrap();
                } else {
                    run_with(&args(&dir, &["--credits"]), &mut transport, &NO_WAIT).unwrap();
                }
                assert_eq!(transport.asked.len(), 1);
                let held = sources::load_all(&path).unwrap().unwrap_or_default();
                let record = held.get(&entity, sources::MUSICBRAINZ).unwrap();
                assert_eq!(
                    held.review_for(record)
                        .map(|review| (review.decision, review.reviewed_at)),
                    decision.map(|decision| (decision, 17))
                );
                let allowed = decision != Some(sources::ReviewDecision::Rejected);
                let expected = if allowed {
                    CreditStatus::Credited
                } else {
                    CreditStatus::Untrusted
                };
                assert_eq!(
                    credit_coverage::editions(&catalog, &held)[0].status,
                    expected,
                    "a valid merged edition lookup must supply its credits"
                );
                let links = held.edition_credit_links(&catalog);
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].trusted, allowed);
                assert_eq!(links[0].credit.artist_name, "Example Producer");
                assert!(
                    held.edition_credit_links(&make_catalog(different))
                        .is_empty(),
                    "another edition of the same album must not inherit these credits"
                );
                let before_retag = std::fs::read(&path).unwrap();
                let mut original_repeat = Canned {
                    answers: Vec::new(),
                    asked: Vec::new(),
                };
                run_with(&args(&dir, &["--credits"]), &mut original_repeat, &NO_WAIT).unwrap();
                assert!(original_repeat.asked.is_empty());
                assert_eq!(std::fs::read(&path).unwrap(), before_retag);
                let retagged = make_catalog(surviving);
                let reloaded = sources::load_all(&path).unwrap().unwrap();
                assert_eq!(
                    credit_coverage::editions(&retagged, &reloaded)[0].status,
                    expected,
                    "retagging to the confirmed canonical edition must reuse saved credits"
                );
                let retagged_links = reloaded.edition_credit_links(&retagged);
                assert_eq!(retagged_links.len(), 1);
                assert_eq!(retagged_links[0].trusted, allowed);
                aede_core::store::save(&retagged, &aede_core::store::catalog_path(&dir)).unwrap();
                let before = std::fs::read(&path).unwrap();
                let mut repeated = Canned {
                    answers: Vec::new(),
                    asked: Vec::new(),
                };
                run_with(&args(&dir, &["--credits"]), &mut repeated, &NO_WAIT).unwrap();
                assert!(repeated.asked.is_empty());
                assert_eq!(std::fs::read(&path).unwrap(), before);
                let mut refresh = Canned {
                    answers: vec![Ok(format!(
                        r#"{{"id":"{surviving}","title":"Example Album"{grouped_facts},"relations":[{{"type":"producer","target-type":"artist","artist":{{"id":"{producer}","name":"Example Producer"}}}}]}}"#
                    ))],
                    asked: Vec::new(),
                };
                if primary {
                    let mut held = sources::load_all(&path).unwrap().unwrap();
                    let targets =
                        super::super::releases::targets(&retagged, &held, &[], &EVERYTHING, true);
                    super::super::releases::run(
                        &mut refresh,
                        &NO_WAIT,
                        &targets,
                        &mut held,
                        &path,
                        0,
                        1,
                    )
                    .unwrap();
                } else {
                    run_with(
                        &args(&dir, &["--credits", "--full"]),
                        &mut refresh,
                        &NO_WAIT,
                    )
                    .unwrap();
                }
                let refreshed = sources::load_all(&path).unwrap().unwrap();
                let record = refreshed.get(&entity, sources::MUSICBRAINZ).unwrap();
                assert_eq!(
                    refreshed
                        .review_for(record)
                        .map(|review| (review.decision, review.reviewed_at)),
                    decision.map(|decision| (decision, 17))
                );
                assert_eq!(
                    credit_coverage::editions(&catalog, &refreshed)[0].status,
                    expected,
                    "refreshing the canonical edition must not lose its earlier verified alias"
                );
                assert_eq!(
                    credit_coverage::editions(&retagged, &refreshed)[0].status,
                    expected
                );
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn credits_fetches_exact_edition_roles_once_without_recording_ids() {
    let dir = sandbox("edition_credits");
    let mut tags = RawTags::default();
    tags.insert("artist", "A Band");
    tags.insert("albumartist", "A Band");
    tags.insert("album", "An Album");
    tags.insert("title", "A Song");
    tags.insert("musicbrainz_albumid", "edition-id");
    tags.insert(
        "musicbrainz_releasegroupid",
        "55555555-5555-4555-8555-555555555555",
    );
    let catalog = build(
        vec![ScannedFile {
            path: "/music/Band/Album/01.flac".into(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".into()],
        1,
        &[],
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).expect("saved");
    let mut transport = Canned {
        answers: vec![Ok(r#"{
          "id":"edition-id","title":"An Album",
          "release-group":{"id":"55555555-5555-4555-8555-555555555555","title":"An Album"},
          "relations":[{"id":"producer-rel","type":"producer","target-type":"artist",
            "artist":{"id":"producer-id","name":"An Album Producer"}}]
        }"#
        .into())],
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut transport, &NO_WAIT).expect("edition fetch");
    assert_eq!(transport.asked.len(), 1);
    assert!(transport.asked[0].contains("/release/edition-id?"));
    let held = sources::load(&sources::sources_path(&dir))
        .unwrap()
        .unwrap();
    let links = held.edition_credit_links(&catalog);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].credit.artist_name, "An Album Producer");
    assert!(links[0].trusted);
    let mut again = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(&args(&dir, &["--credits"]), &mut again, &NO_WAIT).expect("already fetched");
    assert!(again.asked.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn malformed_credit_answers_preserve_completed_work_and_initial_retry_eligibility() {
    use aede_core::credit_coverage::{self, CreditStatus};

    let mut invalid: Vec<(String, String)> =
        ["null", "false", "1", "\"relations\"", "{}", "[null]", "[1]"]
            .into_iter()
            .map(|value| (value.into(), value.into()))
            .collect();
    for target in [
        "null",
        "false",
        "1",
        "\"target\"",
        "[]",
        "{}",
        r#"{"id":""}"#,
    ] {
        let artist =
            format!(r#"[{{"target-type":"artist","type":"performer","artist":{target}}}]"#);
        invalid.push((artist.clone(), artist.clone()));
        invalid.push((
            format!(r#"[{{"target-type":"work","type":"performance","work":{target}}}]"#),
            artist.clone(),
        ));
        invalid.push((format!(r#"[{{"target-type":"work","type":"performance","work":{{"id":"child","relations":{artist}}}}}]"#), artist.clone()));
        invalid.push((format!(r#"[{{"target-type":"work","type":"performance","work":{{"id":"child","relations":[{{"target-type":"work","type":"parts","direction":"backward","work":{target}}}]}}}}]"#), artist));
    }
    for (index, (recording_relations, edition_relations)) in invalid.into_iter().enumerate() {
        let malformed = format!("{recording_relations} / {edition_relations}");
        let dir = sandbox(&format!("credit_answer_shape_{index}"));
        let mut tags = RawTags::default();
        for (key, value) in [
            ("artist", "A Band"),
            ("albumartist", "A Band"),
            ("album", "An Album"),
            ("title", "A Song"),
            ("musicbrainz_recordingid", "recording-id"),
            ("musicbrainz_albumid", "edition-id"),
            (
                "musicbrainz_releasegroupid",
                "55555555-5555-4555-8555-555555555555",
            ),
        ] {
            tags.insert(key, value);
        }
        let catalog = build(
            vec![ScannedFile {
                path: "/isolated/credits/01.flac".into(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            vec!["/isolated".into()],
            1,
            &[],
        );
        let catalog_path = aede_core::store::catalog_path(&dir);
        aede_core::store::save(&catalog, &catalog_path).expect("an identified catalog");
        let catalog_before = std::fs::read(&catalog_path).unwrap();
        let source_path = sources::sources_path(&dir);
        let bad_answers = || {
            vec![
                Ok(format!(
                    r#"{{"id":"recording-id","title":"A Song","relations":{recording_relations}}}"#
                )),
                Ok(format!(
                    r#"{{"id":"edition-id","title":"An Album","release-group":{{"id":"55555555-5555-4555-8555-555555555555","title":"An Album"}},"relations":{edition_relations}}}"#
                )),
            ]
        };
        let mut first = Canned {
            answers: bad_answers(),
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits"]), &mut first, &NO_WAIT)
            .expect("unreadable answers leave these targets alone");
        assert_eq!(first.asked.len(), 2);
        assert!(
            !source_path.exists(),
            "{malformed} is not a completed empty answer"
        );

        let mut retry = Canned {
            answers: vec![
                Ok(r#"{"id":"recording-id","title":"A Song","relations":[
                  {"target-type":"artist","type":"instrument","artist":{"id":"player","name":"A Player"}},
                  {"target-type":"work","type":"performance","work":{"id":"work","title":"A Work","relations":[
                    {"target-type":"artist","type":"composer","artist":{"id":"writer","name":"A Writer"}}
                  ]}}
                ]}"#.into()),
                Ok(r#"{"id":"edition-id","title":"An Album","release-group":{"id":"55555555-5555-4555-8555-555555555555","title":"An Album"},"relations":[
                  {"target-type":"artist","type":"producer","artist":{"id":"producer","name":"A Producer"}}
                ]}"#.into()),
            ],
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits"]), &mut retry, &NO_WAIT)
            .expect("an ordinary run retries the still waiting targets");
        assert_eq!(retry.asked.len(), 2);
        let mut held = sources::load_all(&source_path).unwrap().unwrap();
        let recording = credit_coverage::recordings(&catalog, &held)[0];
        assert_eq!(recording.status, CreditStatus::Credited);
        assert_eq!(recording.recording_credits, 1);
        assert_eq!(recording.work_credits, 1);
        assert_eq!(
            credit_coverage::editions(&catalog, &held)[0].edition_credits,
            1
        );
        for record in &mut held.records {
            if let Facts::Release(facts) = &mut record.facts {
                facts.lookup_mbid = Some("66666666-6666-4666-8666-666666666666".into());
            }
        }
        for record in held.records.clone() {
            held.set_review(sources::SourceReview {
                entity: record.entity(),
                source: record.source,
                source_id: record.source_id,
                decision: sources::ReviewDecision::Accepted,
                reviewed_at: 2,
            });
        }
        sources::save(&held, &source_path).expect("same-identity decisions are retained");
        let reviews_before = held.reviews;
        let source_before = std::fs::read(&source_path).unwrap();

        let mut refresh = Canned {
            answers: bad_answers(),
            asked: Vec::new(),
        };
        run_with(
            &args(&dir, &["--credits", "--full"]),
            &mut refresh,
            &NO_WAIT,
        )
        .expect("a failed refresh preserves its previous snapshots");
        assert_eq!(refresh.asked.len(), 2);
        assert_eq!(std::fs::read(&source_path).unwrap(), source_before);

        let mut no_repeat = Canned {
            answers: Vec::new(),
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits"]), &mut no_repeat, &NO_WAIT)
            .expect("the previous valid snapshots remain completed");
        assert!(no_repeat.asked.is_empty());

        let mut empty = Canned {
            answers: vec![
                Ok(r#"{"id":"recording-id","title":"A Song","relations":[]}"#.into()),
                Ok(r#"{"id":"edition-id","title":"An Album","release-group":{"id":"55555555-5555-4555-8555-555555555555","title":"An Album"},"relations":[]}"#.into()),
            ],
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits", "--full"]), &mut empty, &NO_WAIT)
            .expect("genuine empty answers complete both scopes");
        assert_eq!(empty.asked.len(), 2);
        let held = sources::load_all(&source_path).unwrap().unwrap();
        assert_eq!(held.reviews, reviews_before);
        assert!(
            held.records.iter().any(|record| matches!(
                &record.facts,
                Facts::Release(facts)
                    if facts.lookup_mbid.as_deref() == Some("66666666-6666-4666-8666-666666666666")
            )),
            "edition-credit completion must keep a confirmed group alias"
        );
        assert_eq!(
            credit_coverage::recordings(&catalog, &held)[0].status,
            CreditStatus::Empty
        );
        assert_eq!(
            credit_coverage::editions(&catalog, &held)[0].status,
            CreditStatus::Empty
        );
        let mut no_repeat = Canned {
            answers: Vec::new(),
            asked: Vec::new(),
        };
        run_with(&args(&dir, &["--credits"]), &mut no_repeat, &NO_WAIT)
            .expect("genuine empty answers are not retried");
        assert!(no_repeat.asked.is_empty());
        assert_eq!(std::fs::read(&catalog_path).unwrap(), catalog_before);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Two shelves on the disk, one folder each, so that a folder can be given.
///
/// **Real files**, unlike the reference library above: a folder is read off
/// the filesystem before it is looked up in the catalog, and
/// [`super::Scope::of`] refuses a path that is not there — which is what stops
/// a mistyped folder becoming a silent run over everything.
///
/// **And canonical**, which is not a detail: `std::env::temp_dir()` is
/// `/var/folders/…` on macOS and `/var` is a link to `/private/var`, so a
/// catalog built from the path as handed out is filed under a spelling
/// [`super::super::canonical`] never produces. `aede scan` canonicalizes its
/// roots, so a real catalog never holds that spelling — a fixture that did
/// would be testing a library the program cannot build, and it failed on macOS
/// while passing on Linux, where nothing is a link. See `docs/design/paths.md`.
fn two_shelves(what: &str) -> (aede_core::model::Catalog, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("aede_scope_{}_{what}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let dir = super::super::canonical(&dir);
    let mut files = Vec::new();
    for (artist, album) in [("Alastis", "Revenge"), ("Ozzy Osbourne", "Blizzard of Ozz")] {
        let folder = dir.join(artist);
        std::fs::create_dir_all(&folder).expect("a folder");
        let path = folder.join("01.flac");
        std::fs::write(&path, b"not really audio").expect("a file");
        let mut tags = RawTags::default();
        tags.insert("artist", artist);
        tags.insert("albumartist", artist);
        tags.insert("album", album);
        // Titled after the shelf, so that a track can be told from the other.
        tags.insert("title", artist);
        files.push(ScannedFile {
            path: path.to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let catalog = build(files, vec![dir.to_string_lossy().to_string()], 1, &[]);
    (catalog, dir)
}

#[test]
fn a_positional_that_names_a_folder_is_read_as_one_rather_than_as_a_name() {
    // `aede fetch --lyrics ~/Music/Alastis` used to normalise the whole path
    // into words, match none of them against a title or an artist, and ask
    // about the **entire library** — the swallowed argument this program
    // refuses everywhere else, wearing a slash.
    let (_, dir) = two_shelves("split");
    let shelf = dir.join("Alastis");
    let args = Args::parse(vec![
        "fetch".to_string(),
        shelf.to_string_lossy().to_string(),
        "portishead".to_string(),
    ]);

    let (folders, names) = folders_and_names(&args);
    assert_eq!(folders, vec![shelf.to_string_lossy().to_string()]);
    assert_eq!(
        names,
        vec!["portishead".to_string()],
        "a word that is not on the disk is still a name"
    );
}

#[test]
fn a_folder_reaches_the_tracks_the_album_and_the_artist_under_it() {
    // One walk of the catalog answers for every pass, so what a folder means
    // cannot differ between them.
    let (catalog, dir) = two_shelves("reach");
    let scope = Scope::of(
        &catalog,
        &[dir.join("Alastis").to_string_lossy().to_string()],
    )
    .expect("a folder the catalog holds");

    let artist = |name: &str| {
        catalog
            .artists
            .iter()
            .find(|a| a.name == name)
            .expect("the artist")
    };
    assert!(scope.has_artist(&artist("Alastis").key));
    assert!(!scope.has_artist(&artist("Ozzy Osbourne").key));

    let release = |title: &str| {
        catalog
            .releases
            .iter()
            .find(|r| r.title == title)
            .expect("the record")
    };
    assert!(scope.has_release(release("Revenge").id));
    assert!(!scope.has_release(release("Blizzard of Ozz").id));

    let track = |title: &str| {
        catalog
            .tracks
            .iter()
            .find(|t| t.title == title)
            .expect("the track")
    };
    assert!(scope.has_track(track("Alastis").id));
    assert!(!scope.has_track(track("Ozzy Osbourne").id));
}

#[test]
fn no_folder_at_all_reaches_everything() {
    // The distinction the `Option` inside the scope exists for: "no folder was
    // given" reaches the whole library, "a folder that holds nothing" reaches
    // nothing — and reading the first as the second would turn every ordinary
    // fetch into a run with nothing to do.
    let (catalog, _) = two_shelves("everything");
    let scope = Scope::of(&catalog, &[]).expect("no folder is not a refusal");
    assert!(scope.is_empty());
    for artist in &catalog.artists {
        assert!(scope.has_artist(&artist.key));
    }
    for release in &catalog.releases {
        assert!(scope.has_release(release.id));
    }
}

#[test]
fn a_folder_the_catalog_has_never_seen_is_refused_rather_than_run_over() {
    // The same refusal `check` and `playlist` make, and for the same reason: a
    // folder added since the last scan would otherwise produce a cheerful
    // "nothing to ask about" that reads as *your library is done*.
    let (catalog, dir) = two_shelves("unknown");
    let outside = dir.join("Portishead");
    std::fs::create_dir_all(&outside).expect("a folder");

    let refused = Scope::of(&catalog, &[outside.to_string_lossy().to_string()]);
    assert!(refused.is_err(), "a folder no file of the catalog is under");
}

#[test]
fn the_album_half_of_an_ordinary_fetch_honours_a_folder() {
    // Both halves of the ordinary run, not only the artists: `aede fetch
    // ~/Music/Alastis` that asked about every album of the library would be
    // answering a question nobody typed.
    let (catalog, dir) = two_shelves("albums");
    let scope = Scope::of(
        &catalog,
        &[dir.join("Alastis").to_string_lossy().to_string()],
    )
    .expect("a folder the catalog holds");
    let held = sources::Sources::default();

    let narrowed = crate::commands::releases::targets(&catalog, &held, &[], &scope, false);
    assert_eq!(
        crate::commands::releases::names(&narrowed).collect::<Vec<_>>(),
        vec!["Revenge"]
    );
    let whole = crate::commands::releases::targets(&catalog, &held, &[], &EVERYTHING, false);
    assert_eq!(whole.len(), 2, "and without a folder, both records");
}

#[test]
fn discogs_is_an_explicit_pass_after_label_identification_and_wikipedia() {
    let dir = sandbox("discogs_pass_order");
    let passes = Pass::asked_for(&args(&dir, &["--discogs", "--summaries", "--labels"]));
    assert_eq!(passes, vec![Pass::Labels, Pass::Summaries, Pass::Discogs]);
    assert!(!Pass::asked_for(&args(&dir, &["--labels"])).contains(&Pass::Discogs));
    assert!(Pass::Discogs.needs_the_catalog());
}

#[test]
fn unavailable_discogs_labels_do_not_skip_the_following_cover_pass() {
    struct Trace(Vec<String>);
    impl super::super::fetch::Ask for Trace {
        fn get_json(&mut self, url: &str) -> Result<Json, super::super::fetch::Refusal> {
            self.0.push(url.into());
            Ok(aede_core::json::parse(r#"{"id":"ce105941-dc16-4291-9d18-5fed7edc5e5a","name":"Roadracer Records","relations":[]}"#).unwrap())
        }
        fn get_bytes(&mut self, _: &str) -> Result<Vec<u8>, super::super::fetch::Refusal> {
            panic!("metadata-only test must not download images")
        }
    }
    let dir = sandbox("following_artwork");
    let mut catalog = super::super::load(&args(&dir, &[])).unwrap();
    catalog.labels.push(aede_core::model::Label {
        id: 0,
        key: "roadracer records".into(),
        name: "Roadracer Records".into(),
        mbid: None,
    });
    catalog.labels[0].mbid = Some("ce105941-dc16-4291-9d18-5fed7edc5e5a".into());
    catalog.releases[0].mbid = Some("11111111-1111-4111-8111-111111111111".into());
    let target = sources::sources_path(&dir);
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
    let mut transport = Trace(Vec::new());
    let result = second_passes(
        &[Pass::Discogs, Pass::Covers],
        &crate::args::Args::default(),
        &mut transport,
        &[],
        &mut sources::Sources::default(),
        &target,
        target.parent().unwrap(),
        &asked,
        Some(&catalog),
    );
    assert!(
        result.is_err(),
        "unavailable labels must remain visible in the final outcome"
    );
    assert!(
        transport
            .0
            .iter()
            .any(|url| url.contains("coverartarchive.org")),
        "artwork was skipped: {:?}",
        transport.0
    );
    std::fs::remove_dir_all(target.parent().unwrap()).unwrap();
}

#[test]
fn recording_credit_names_match_each_local_placement_title() {
    let dir = sandbox("placement_title_filter");
    let files = [
        ("Original", "Original title"),
        ("Reissue", "Alternate title"),
    ]
    .into_iter()
    .map(|(album, title)| {
        let mut tags = RawTags::default();
        for (key, value) in [
            ("artist", "A Player"),
            ("albumartist", "A Player"),
            ("album", album),
            ("title", title),
            ("musicbrainz_recordingid", "shared-recording"),
        ] {
            tags.insert(key, value);
        }
        ScannedFile {
            path: format!("/music/{album}/01.flac"),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }
    })
    .collect();
    let catalog = build(files, vec!["/music".into()], 1, &[]);
    assert_eq!(catalog.recordings.len(), 1);
    assert_eq!(catalog.recordings[0].title, "Original title");
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
    let before = std::fs::read(aede_core::store::catalog_path(&dir)).unwrap();
    let mut transport = Canned {
        answers: vec![Ok(
            r#"{"id":"shared-recording","title":"Original title","relations":[]}"#.into(),
        )],
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--credits", "Alternate title"]),
        &mut transport,
        &NO_WAIT,
    )
    .unwrap();
    assert_eq!(transport.asked.len(), 1);
    let held = sources::load_all(&sources::sources_path(&dir))
        .unwrap()
        .unwrap();
    assert_eq!(held.records.len(), 1);
    assert_eq!(held.records[0].key, "/music/Reissue/01.flac");
    let mut repeated = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--credits", "Original title"]),
        &mut repeated,
        &NO_WAIT,
    )
    .unwrap();
    assert!(
        repeated.asked.is_empty(),
        "the recording cache stays shared"
    );
    assert_eq!(
        std::fs::read(aede_core::store::catalog_path(&dir)).unwrap(),
        before
    );
}

#[test]
fn an_exact_artist_summary_scope_excludes_prefix_artists_and_labels() {
    let dir = sandbox("exact_summary");
    let mut catalog = super::super::load(&args(&dir, &[])).unwrap();
    let artist = catalog.artists[0].clone();
    let reference = EntityRef::of(&catalog, EntityKind::Artist, artist.id)
        .unwrap()
        .to_token();
    let mut other = artist.clone();
    other.id = artist.id + 1;
    other.name = format!("{} Ensemble", artist.name);
    catalog.artists.push(other);
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
    let mut held = sources::Sources::default();
    for name in [artist.name.as_str(), &format!("{} Ensemble", artist.name)] {
        held.set(SourceRecord {
            key: text::normalize(name),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("11111111-1111-4111-8111-111111111111".into()),
            fetched_at: 1,
            confidence: sources::Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                wikidata: Some("https://www.wikidata.org/wiki/Q123".into()),
                ..Default::default()
            }),
        });
    }
    held.set(SourceRecord {
        key: text::normalize(&artist.name),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("label-id".into()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: Facts::Label(sources::LabelFacts {
            wikidata: Some("https://www.wikidata.org/wiki/Q456".into()),
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).unwrap();
    let option = format!("--artist-ref={reference}");
    let mut transport = Canned {
        answers: vec![Ok(r#"{"entities":{"Q123":{"sitelinks":{}}}}"#.into())],
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--summaries", &option, "--yes"]),
        &mut transport,
        &NO_WAIT,
    )
    .unwrap();
    assert_eq!(
        transport.asked,
        ["https://www.wikidata.org/wiki/Special:EntityData/Q123.json"]
    );
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    for options in [
        vec!["--summaries", &option, "Miles"],
        vec!["--covers", &option],
        vec!["--summaries", "--artist-ref=release:wrong"],
        vec!["--summaries", "--artist-ref=artist:absent"],
    ] {
        assert!(run_with(&args(&dir, &options), &mut transport, &NO_WAIT).is_err());
    }
    assert!(transport.asked.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn exact_artist_scope_names_its_subject_in_empty_and_preview_messages() {
    let scope = Scope {
        holds: Some(Held {
            artists: ["example ensemble".into()].into_iter().collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(narrowing(&[], &scope), "example ensemble");
    assert_eq!(
        nothing_named(&[], &scope, 0),
        "nothing here matches example ensemble"
    );
}

#[test]
fn a_valid_mbid_artist_reference_fetches_the_same_biography_as_its_catalogue_key() {
    let dir = sandbox("summary_mbid_reference");
    let mut catalog = super::super::load(&args(&dir, &[])).unwrap();
    let mbid = "11111111-1111-4111-8111-111111111111";
    catalog.artists[0].mbid = Some(mbid.into());
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
    let mut held = sources::Sources::default();
    held.set(SourceRecord {
        key: catalog.artists[0].key.clone(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some(mbid.into()),
        fetched_at: 1,
        confidence: sources::Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            wikidata: Some("https://www.wikidata.org/wiki/Q123".into()),
            ..Default::default()
        }),
    });
    sources::save(&held, &sources::sources_path(&dir)).unwrap();
    let selector = format!("--artist-ref=artist:mbid:{mbid}");
    let reference = EntityRef::parse_token(&selector[13..]).unwrap();
    assert_eq!(reference.resolve(&catalog), Some(catalog.artists[0].id));
    let mut transport = Canned {
        answers: vec![Ok(r#"{"entities":{"Q123":{"sitelinks":{}}}}"#.into())],
        asked: Vec::new(),
    };
    run_with(
        &args(&dir, &["--summaries", &selector, "--yes"]),
        &mut transport,
        &NO_WAIT,
    )
    .unwrap();
    assert_eq!(
        transport.asked,
        ["https://www.wikidata.org/wiki/Special:EntityData/Q123.json"]
    );
    std::fs::remove_dir_all(dir).unwrap();
}
