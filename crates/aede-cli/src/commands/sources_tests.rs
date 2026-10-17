//! What the `sources` command shows and stores.
//!
//! In its own file from the start, under the rule `fetch` established: the
//! module is declared in `sources.rs` with `#[path]`, so these are its child
//! module and reach its private functions through `use super::*`.
//!
//! The display is what most of this covers, because the display is where this
//! command's mistakes have actually been: a column that claimed the tags were
//! silent when there was no tag to be silent, and a section that rendered
//! "(no results)" for a source that had answered and held nothing.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{ArtistFacts, ReleaseFacts};
use aede_core::tags::RawTags;

#[test]
fn unreadable_facts_imports_preserve_existing_snapshots_and_all_other_stores() {
    use aede_core::json::{self, Json};
    use aede_core::sources::{LabelFacts, TrackFacts};

    let dir = sandbox("facts_container_import");
    let audio = dir.join("music/01.flac");
    std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&audio, bytes).unwrap();
    let mut catalog = catalog_with("Jazz", "Columbia");
    catalog.files[0].path = audio.to_string_lossy().into_owned();
    aede_core::store::save(&catalog, &dir.join("catalog.json")).unwrap();
    aede_core::user::save(&Default::default(), &dir.join("user.json")).unwrap();
    aede_core::conclusions::save(&Default::default(), &dir.join("conclusions.json")).unwrap();

    let mut held = Sources::default();
    for (entity, facts) in [
        (
            artist_of(&catalog),
            Facts::Artist(ArtistFacts {
                area: Some("Stored area".into()),
                ..Default::default()
            }),
        ),
        (
            release_of(&catalog),
            Facts::Release(ReleaseFacts {
                label: Some("Stored label".into()),
                ..Default::default()
            }),
        ),
        (
            EntityRef::of(&catalog, EntityKind::Track, 0).unwrap(),
            Facts::Track(TrackFacts {
                title: Some("Stored title".into()),
                ..Default::default()
            }),
        ),
        (
            EntityRef::of(&catalog, EntityKind::Label, 0).unwrap(),
            Facts::Label(LabelFacts {
                discogs: Some("https://www.discogs.com/label/123".into()),
                ..Default::default()
            }),
        ),
    ] {
        held.set(SourceRecord {
            key: entity.key,
            source: "manual".into(),
            source_id: None,
            fetched_at: 123,
            confidence: Confidence::Identified,
            facts,
        });
    }
    sources::save(&held, &sources::sources_path(&dir)).unwrap();
    let snapshot = || {
        [
            "catalog.json",
            "sources.json",
            "user.json",
            "conclusions.json",
        ]
        .map(|name| std::fs::read(dir.join(name)).unwrap())
    };
    let before = snapshot();
    let input = dir.join("incoming.json");
    let command = args(&dir, &["--import", input.to_str().unwrap()]);
    for index in 0..held.records.len() {
        for malformed in [
            None,
            Some(Json::Null),
            Some(Json::Bool(false)),
            Some(1u32.into()),
            Some("facts".into()),
            Some(Json::Arr(vec![])),
        ] {
            let mut incoming = sources::to_json(&held);
            let mut records = incoming.get("records").unwrap().as_arr().unwrap().to_vec();
            match malformed {
                Some(ref facts) => records[index].set("facts", facts.clone()),
                None => {
                    let Json::Obj(fields) = &mut records[index] else {
                        panic!("the serialized record is an object");
                    };
                    fields.remove("facts");
                }
            }
            incoming.set("records", Json::Arr(records));
            std::fs::write(&input, incoming.to_string_pretty()).unwrap();
            let error = sources(&command).expect_err("unreadable facts cannot replace a snapshot");
            assert!(error.to_string().contains("facts"), "{error}");
            assert_eq!(snapshot(), before, "row {index}: {malformed:?}");
            assert_eq!(std::fs::read(&audio).unwrap(), bytes);
        }
    }
    for version in [1, sources::SOURCES_FORMAT_VERSION] {
        let mut empty = sources::to_json(&held);
        empty.set("format_version", version.into());
        let mut records = empty.get("records").unwrap().as_arr().unwrap().to_vec();
        for record in &mut records {
            record.set("facts", Json::obj());
        }
        empty.set("records", Json::Arr(records));
        std::fs::write(&input, empty.to_string_pretty()).unwrap();
        sources(&command).unwrap();
        let restored = sources::load_all(&sources::sources_path(&dir))
            .unwrap()
            .unwrap();
        assert_eq!(
            restored,
            sources::from_json(&json::parse(&empty.to_string_pretty()).unwrap()).unwrap()
        );
        let after = snapshot();
        for index in [0, 2, 3] {
            assert_eq!(after[index], before[index]);
        }
        assert_eq!(std::fs::read(&audio).unwrap(), bytes);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn discogs_layout_keeps_paragraphs_and_emphasizes_a_wrapped_label_code() {
    let text = "Originally a Dutch label.\r\n[b]Label Code: LC 9231 / LC 09231[/b]\r\nPlease use [l=Roadrunner Productions].\r\n\r\nThe biography starts here.";
    let plain = aede_core::discogs::render_profile(text, &Default::default(), &Default::default());
    assert_eq!(
        discogs_profile_lines(&plain, 25),
        vec![
            "Originally a Dutch label.".to_string(),
            ui::bold("Label Code: LC 9231 / LC"),
            ui::bold("09231"),
            "Please use Roadrunner".to_string(),
            "Productions.".to_string(),
            String::new(),
            "The biography starts".to_string(),
            "here.".to_string(),
        ]
    );
}

/// A one-album, one-artist catalog whose tags say what is given.
fn catalog_with(genre: &str, label: &str) -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("date", "1959");
    tags.insert("genre", genre);
    tags.insert("label", label);
    build(
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
    )
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
    let dir = std::env::temp_dir().join(format!("aede_sources_{}_{name}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a data folder");
    dir
}

fn saved(name: &str, catalog: &Catalog) -> std::path::PathBuf {
    let dir = sandbox(name);
    aede_core::store::save(catalog, &aede_core::store::catalog_path(&dir)).expect("a catalog");
    dir
}

fn args(dir: &std::path::Path, extra: &[&str]) -> Args {
    let mut raw = vec!["sources".to_string(), format!("--data={}", dir.display())];
    raw.extend(extra.iter().map(|s| s.to_string()));
    Args::parse(raw)
}

fn artist_of(catalog: &Catalog) -> EntityRef {
    EntityRef::of(catalog, EntityKind::Artist, 0).expect("an artist")
}

fn release_of(catalog: &Catalog) -> EntityRef {
    EntityRef::of(catalog, EntityKind::Release, 0).expect("a release")
}

#[test]
fn a_field_the_tags_cannot_hold_is_not_a_field_they_are_missing() {
    // The bug this pins: artist fields were shown against a "Your tags" column
    // reading "nothing in your tags", which sends the reader looking for a tag
    // to fill that has never existed. There is no widely used tag for where an
    // artist is from, so the right answer is no verdict at all.
    let catalog = catalog_with("Jazz", "Columbia");
    let rows = compared(
        &catalog,
        &artist_of(&catalog),
        &Facts::Artist(ArtistFacts {
            area: Some("United States".to_string()),
            kind: Some("Person".to_string()),
            ..Default::default()
        }),
    );
    assert!(
        rows.iter().all(|(_, _, verdict)| verdict.is_none()),
        "no artist field is judged against a tag: {rows:?}"
    );
    assert!(rows.iter().any(|(field, _, _)| *field == "from"));
    assert!(
        !rows.iter().any(|(field, _, _)| *field == "country"),
        "an area is a country, a city or a region — \"country\" was the wrong word"
    );
}

#[test]
fn a_release_field_is_judged_against_the_tag_that_answers_it() {
    // The other half: a release does have tag counterparts, which is what
    // makes agreement and disagreement observable at all.
    let catalog = catalog_with("Jazz", "Columbia");
    let rows = compared(
        &catalog,
        &release_of(&catalog),
        &Facts::Release(ReleaseFacts {
            primary_type: Some("Album".to_string()),
            first_released: Some("1959-08-17".to_string()),
            label: Some("Blue Note".to_string()),
            label_mbid: None,
            secondary_types: vec![],
            cover_art: None,
            ..Default::default()
        }),
    );

    let verdict_for = |name: &str| {
        rows.iter()
            .find(|(field, _, _)| *field == name)
            .and_then(|(_, _, v)| v.clone())
    };

    // No RELEASETYPE tag here: the source adds rather than contradicts.
    assert_eq!(verdict_for("release type"), Some(Verdict::NothingToCompare));
    // A full date against a bare year is agreement, not a disagreement — the
    // false alarm that would otherwise land on nearly every album.
    assert_eq!(verdict_for("first released"), Some(Verdict::Agrees));
    // And a real difference names both sides.
    assert_eq!(
        verdict_for("label"),
        Some(Verdict::Differs {
            theirs: "Blue Note".to_string(),
            yours: "Columbia".to_string()
        })
    );
}

#[test]
fn a_genre_is_compared_with_the_one_your_files_carry() {
    // A genre lives on the tracks, not on the artist, so "what do my files
    // call this artist" is answered by the files they appear on.
    let catalog = catalog_with("Jazz", "Columbia");
    let entity = artist_of(&catalog);
    assert_eq!(tags_of_artist(&catalog, &entity, "genre"), vec!["Jazz"]);

    let genres_of = |theirs: &[&str]| {
        compared(
            &catalog,
            &entity,
            &Facts::Artist(ArtistFacts {
                genres: theirs.iter().map(|g| g.to_string()).collect(),
                ..Default::default()
            }),
        )
        .into_iter()
        .find(|(field, _, _)| *field == "genres")
        .expect("a genres row")
    };

    let agreeing = genres_of(&["jazz", "cool jazz"]);
    assert_eq!(agreeing.1, "jazz, cool jazz", "all of them are shown");
    assert_eq!(
        agreeing.2,
        Some(Verdict::Agrees),
        "case does not matter, and a shared name is agreement"
    );

    // The false alarm this replaced: the top genre was compared, as a string,
    // against the whole tag. A tag naming two genres, one of which the source
    // also names, is not a contradiction.
    let overlapping = genres_of(&["cool jazz", "jazz"]);
    assert_eq!(
        overlapping.2,
        Some(Verdict::Agrees),
        "the shared name need not be first on either side"
    );

    let disjoint = genres_of(&["techno", "house"]);
    assert!(
        matches!(disjoint.2, Some(Verdict::Differs { .. })),
        "and two sets with nothing in common still differ: {:?}",
        disjoint.2
    );
}

#[test]
fn every_source_is_shown_the_address_its_answer_came_from() {
    // The identifier was stored from the first version and never displayed.
    // Asked for the MusicBrainz id of an artist there was nowhere in this
    // program to find it, and the nearest thing on screen was the Wikidata
    // link — a different identifier that looks enough like an answer to send
    // somebody off with a query that cannot work.
    let artist = SourceRecord {
        key: "marilyn manson".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("c98ff0e1-a92a-4a24-8f21-1a6b1e0b7c0f".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts::default()),
    };
    let line = address_of(&artist).expect("an address");
    assert_eq!(
        line, "https://musicbrainz.org/artist/c98ff0e1-a92a-4a24-8f21-1a6b1e0b7c0f",
        "the identifier is inside the address, so it can still be copied for \
         a query — and the page it opens is where the data is corrected"
    );

    // An album is stored under its *release group*, not its edition, so the
    // path has to say so: the edition path would 404 on the one page a reader
    // came to open.
    let album = SourceRecord {
        key: "manson|antichrist superstar|/music".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("aa11".to_string()),
        facts: Facts::Release(Default::default()),
        ..artist
    };
    assert_eq!(
        address_of(&album).as_deref(),
        Some("https://musicbrainz.org/release-group/aa11")
    );

    // Wikipedia records carry a Wikidata entity, which lives elsewhere.
    let summary = SourceRecord {
        source: "wikipedia".to_string(),
        source_id: Some("Q485893".to_string()),
        ..artist.clone()
    };
    assert_eq!(
        address_of(&summary).as_deref(),
        Some("https://www.wikidata.org/wiki/Q485893")
    );

    // A source this build knows nothing about still has an identifier, and the
    // identifier is the useful half. Inventing an address would be worse.
    let plugin = SourceRecord {
        source: "some-plugin".to_string(),
        source_id: Some("xyz-1".to_string()),
        ..artist.clone()
    };
    assert_eq!(address_of(&plugin).as_deref(), Some("xyz-1"));

    // Nothing was identified, so there is nothing to point at.
    let manual = SourceRecord {
        source: "manual".to_string(),
        source_id: None,
        ..artist
    };
    assert_eq!(address_of(&manual), None);
}

#[test]
fn what_a_record_says_reads_as_a_sentence_or_says_it_holds_nothing() {
    let full = says(&Facts::Artist(ArtistFacts {
        kind: Some("Group".to_string()),
        area: Some("United States".to_string()),
        began: Some("1989".to_string()),
        active: Some(true),
        ..Default::default()
    }));
    assert!(full.contains("Group"), "{full}");
    assert!(full.contains("1989–"), "an open period stays open: {full}");
    assert!(full.contains("still active"), "{full}");

    let ended = says(&Facts::Artist(ArtistFacts {
        began: Some("1926".to_string()),
        ended: Some("1991".to_string()),
        active: Some(false),
        ..Default::default()
    }));
    assert!(ended.contains("1926–1991"), "{ended}");

    // An answer holding nothing is a real answer: printing an empty cell would
    // make it look like a missing row instead.
    let empty = says(&Facts::Artist(ArtistFacts::default()));
    assert!(empty.contains("nothing"), "{empty}");
}

#[test]
fn a_search_and_a_lookup_do_not_read_the_same() {
    // The distinction the whole layer turns on, in the one place a reader
    // sees it.
    assert_eq!(
        super::super::source_status(Confidence::Identified, None, true),
        "identified"
    );
    assert_eq!(
        super::super::source_status(Confidence::matched(88), None, false),
        "matched 88% · pending review"
    );
}

#[test]
fn a_template_carries_the_keys_and_an_import_takes_them_back() {
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("roundtrip", &catalog);
    let written = dir.join("template.json");

    template(&args(
        &dir,
        &[
            "--template",
            "--source=manual",
            "--output",
            written.to_str().unwrap(),
            "Kind of Blue",
        ],
    ))
    .expect("a template");

    let text = std::fs::read_to_string(&written).expect("a document");
    assert!(text.contains("\"source\": \"manual\""), "{text}");
    assert!(text.contains("\"entity\": \"release:"), "{text}");
    assert!(text.contains("\"primary_type\": null"), "{text}");

    // Filled in and taken back, it lands under the source it names.
    std::fs::write(
        &written,
        text.replace("\"primary_type\": null", "\"primary_type\": \"Album\""),
    )
    .unwrap();
    import(
        &args(&dir, &[&format!("--import={}", written.display())]),
        &sources::sources_path(&dir),
    )
    .expect("an import");

    let held = sources::load(&sources::sources_path(&dir))
        .expect("readable")
        .expect("a layer");
    assert_eq!(held.records.len(), 1);
    assert_eq!(held.records[0].source, "manual");
    match &held.records[0].facts {
        Facts::Release(r) => assert_eq!(r.primary_type.as_deref(), Some("Album")),
        other => panic!("expected release facts, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn naming_something_the_catalog_does_not_hold_is_refused() {
    // Writing an empty document and reporting success would look like an
    // answer. It is not one.
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("nomatch", &catalog);
    let target = dir.join("template.json");
    let refused = template(&args(
        &dir,
        &[
            "--template",
            "--output",
            target.to_str().unwrap(),
            "an album nobody owns",
        ],
    ))
    .expect_err("refused");
    assert!(
        refused
            .to_string()
            .contains("nothing in this catalog matches")
    );
    assert!(!target.exists(), "and no file was written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn forgetting_a_source_nobody_stored_is_an_error_and_removes_nothing() {
    // An instruction that cannot be honoured is refused, not treated as a
    // no-op — and above all it must not take the other sources with it.
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("forget", &catalog);
    let path = sources::sources_path(&dir);

    let mut held = Sources::default();
    held.set(SourceRecord {
        key: artist_of(&catalog).key,
        source: sources::MUSICBRAINZ.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts::default()),
    });
    sources::save(&held, &path).expect("saved");

    let refused = forget(&args(&dir, &["--forget", "--source=discogs"]), &path)
        .expect_err("an unknown source is refused");
    assert!(refused.to_string().contains("no source named"), "{refused}");

    let after = sources::load(&path).expect("readable").expect("a layer");
    assert_eq!(after.records.len(), 1, "nothing was removed on the way out");

    // And forgetting the one that is there empties it.
    forget(&args(&dir, &["--forget"]), &path).expect("forgotten");
    let empty = sources::load(&path).expect("readable").expect("a layer");
    assert!(empty.records.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn duplicate_review_import_is_refused_before_existing_evidence_changes() {
    use aede_core::sources::{Prose, ReviewDecision, SourceReview};

    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("duplicate_review_import", &catalog);
    let path = sources::sources_path(&dir);
    let review = SourceReview {
        entity: artist_of(&catalog),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some("candidate-id".into()),
        decision: ReviewDecision::Rejected,
        reviewed_at: 200,
    };
    let old_profile = SourceRecord {
        key: "columbia".into(),
        source: aede_core::discogs::SOURCE.into(),
        source_id: Some("1".into()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts {
            summary: Some(Prose {
                text: "Retained expired profile.".into(),
                url: "https://www.discogs.com/label/1".into(),
                lang: "und".into(),
                licence: "CC0".into(),
            }),
            ..Default::default()
        }),
    };
    let held = Sources {
        records: vec![old_profile.clone()],
        reviews: vec![review.clone()],
        ..Default::default()
    };
    sources::save(&held, &path).unwrap();
    let snapshots = || {
        [
            "catalog.json",
            "sources.json",
            "user.json",
            "conclusions.json",
        ]
        .map(|name| std::fs::read(dir.join(name)).ok())
    };
    let before = snapshots();
    let incoming = Sources {
        reviews: vec![SourceReview {
            decision: ReviewDecision::Accepted,
            reviewed_at: 100,
            ..review
        }],
        ..Default::default()
    };
    let mut duplicate = sources::to_json(&incoming);
    let row = duplicate.get("reviews").unwrap().as_arr().unwrap()[0].clone();
    duplicate.set(
        "reviews",
        aede_core::json::Json::Arr(vec![row.clone(), row]),
    );
    let input = dir.join("incoming.json");
    std::fs::write(&input, duplicate.to_string_pretty()).unwrap();
    let command = args(&dir, &["--import", input.to_str().unwrap()]);
    let error = sources(&command).expect_err("duplicate exact review keys are ambiguous");
    assert!(
        error.to_string().contains("duplicate source reviews"),
        "{error}"
    );
    assert_eq!(snapshots(), before);

    // A separate import with one decision deliberately replaces the prior
    // exact decision. Other retained evidence must survive that valid write.
    std::fs::write(&input, sources::to_json(&incoming).to_string_pretty()).unwrap();
    sources(&command).unwrap();
    let after = sources::load_all(&path).unwrap().unwrap();
    assert_eq!(after.records, [old_profile]);
    assert_eq!(after.reviews, incoming.reviews);
    let after_bytes = snapshots();
    for index in [0, 2, 3] {
        assert_eq!(after_bytes[index], before[index]);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn there_is_nothing_to_export_before_anything_is_fetched() {
    let catalog = catalog_with("Jazz", "Columbia");
    let dir = saved("export", &catalog);
    let path = sources::sources_path(&dir);
    let refused = export(&args(&dir, &["--export"]), &path).expect_err("refused");
    assert!(refused.to_string().contains("nothing has been fetched"));
    assert!(
        refused.to_string().contains("--template"),
        "and says what to do instead: {refused}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn importing_unreadable_completed_credits_preserves_every_store() {
    use aede_core::sources::{CreditLink, TrackFacts};
    use aede_core::{
        conclusions,
        json::{self, Json},
        user,
    };
    let dir = sandbox("completed_credit_admission");
    let audio = dir.join("music/01.flac");
    std::fs::create_dir_all(audio.parent().unwrap()).unwrap();
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&audio, bytes).unwrap();
    let mut tags = RawTags::default();
    for (key, value) in [
        ("artist", "A Player"),
        ("albumartist", "A Player"),
        ("album", "An Album"),
        ("title", "A Recording"),
        ("musicbrainz_recordingid", "recording-id"),
        ("musicbrainz_albumid", "edition-id"),
        ("musicbrainz_releasegroupid", "group-id"),
    ] {
        tags.insert(key, value);
    }
    let catalog = build(
        vec![ScannedFile {
            path: audio.to_string_lossy().into_owned(),
            size: bytes.len() as u64,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![dir.to_string_lossy().into_owned()],
        1,
        &[],
    );
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&dir)).unwrap();
    let mut personal = user::UserData::default();
    personal
        .entry(
            user::LOCAL_USER,
            &EntityRef::of(&catalog, EntityKind::Track, 0).unwrap(),
            1,
        )
        .note = Some("Retained personal note".into());
    user::save(&personal, &user::user_path(&dir)).unwrap();
    conclusions::save(
        &conclusions::Conclusions::default(),
        &conclusions::conclusions_path(&dir),
    )
    .unwrap();
    let credit = CreditLink {
        artist_mbid: "writer-id".into(),
        artist_name: "A Writer".into(),
        role: "composer".into(),
        relation_id: Some("relationship-id".into()),
        role_id: None,
        direction: None,
        credited_as: None,
        attributes: vec![],
        began: None,
        ended: None,
        over: None,
        order: None,
    };
    let held = Sources {
        records: vec![
            SourceRecord {
                key: audio.to_string_lossy().into_owned(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some("recording-id".into()),
                fetched_at: 1,
                confidence: Confidence::Identified,
                facts: Facts::Track(TrackFacts {
                    recording: Some("recording-id".into()),
                    credits: vec![credit.clone()],
                    relationships_complete: true,
                    ..Default::default()
                }),
            },
            SourceRecord {
                key: release_of(&catalog).key,
                source: sources::MUSICBRAINZ.into(),
                source_id: Some("group-id".into()),
                fetched_at: 1,
                confidence: Confidence::Identified,
                facts: Facts::Release(ReleaseFacts {
                    edition_mbid: Some("edition-id".into()),
                    credits: vec![credit],
                    relationships_complete: true,
                    ..Default::default()
                }),
            },
        ],
        ..Default::default()
    };
    sources::save(&held, &sources::sources_path(&dir)).unwrap();
    let snapshot = || {
        [
            "catalog.json",
            "sources.json",
            "user.json",
            "conclusions.json",
        ]
        .map(|name| std::fs::read(dir.join(name)).unwrap())
    };
    let before = snapshot();
    let input = dir.join("incoming.json");
    let command = args(&dir, &["--import", input.to_str().unwrap()]);
    for (row, body) in [
        (0, r#"{"relationships_complete":true,"credits":{}}"#),
        (0, r#"{"relationships_complete":true,"credits":[null]}"#),
        (0, r#"{"relationships_complete":true,"works":{}}"#),
        (0, r#"{"relationships_complete":true,"works":[{}]}"#),
        (
            0,
            r#"{"relationships_complete":true,"works":[{"mbid":"work-id","credits":{}}]}"#,
        ),
        (1, r#"{"relationships_complete":true,"credits":{}}"#),
    ] {
        let mut incoming = sources::to_json(&held);
        let mut records = incoming.get("records").unwrap().as_arr().unwrap().to_vec();
        records[row].set("facts", json::parse(body).unwrap());
        incoming.set("records", Json::Arr(records));
        std::fs::write(&input, incoming.to_string_pretty()).unwrap();
        let error =
            sources(&command).expect_err("an unreadable list cannot complete a credit lookup");
        assert!(error.to_string().contains("completed credit"), "{error}");
        assert_eq!(snapshot(), before, "{body}");
        assert_eq!(std::fs::read(&audio).unwrap(), bytes);
    }
    // A complete empty answer is still a valid replacement, unlike unreadable rows.
    let mut empty = held.clone();
    if let Facts::Track(facts) = &mut empty.records[0].facts {
        facts.credits.clear();
    }
    if let Facts::Release(facts) = &mut empty.records[1].facts {
        facts.credits.clear();
    }
    std::fs::write(&input, sources::to_json(&empty).to_string_pretty()).unwrap();
    sources(&command).unwrap();
    assert_eq!(
        sources::load_all(&sources::sources_path(&dir))
            .unwrap()
            .unwrap(),
        empty
    );
    let after = snapshot();
    for index in [0, 2, 3] {
        assert_eq!(after[index], before[index]);
    }
    assert_eq!(std::fs::read(audio).unwrap(), bytes);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_edition_without_a_group_keeps_its_own_musicbrainz_address() {
    let response = aede_core::json::parse(
        r#"{"id":"11111111-1111-1111-1111-111111111111","title":"A Reviewed Edition","relations":[]}"#,
    )
    .unwrap();
    let candidate = aede_core::musicbrainz::release(&response).unwrap();
    assert_eq!(candidate.facts.edition_mbid.as_ref(), Some(&candidate.mbid));
    let record = SourceRecord {
        key: "reviewed edition".into(),
        source: sources::MUSICBRAINZ.into(),
        source_id: Some(candidate.mbid),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(candidate.facts),
    };
    assert_eq!(
        address_of(&record).as_deref(),
        Some("https://musicbrainz.org/release/11111111-1111-1111-1111-111111111111")
    );
}

#[test]
fn source_lists_mark_biographies_without_exposing_unattributed_extracts() {
    let prose = sources::Prose {
        text: "Licensed biography words that require their original credit.".into(),
        url: "https://example.org/credited-biography".into(),
        lang: "en".into(),
        licence: "CC BY-SA 4.0".into(),
    };
    let catalog = catalog_with("Jazz", "Columbia");
    for (facts, marker) in [
        (
            Facts::Artist(ArtistFacts {
                summary: Some(prose.clone()),
                ..Default::default()
            }),
            "biography",
        ),
        (
            Facts::Label(LabelFacts {
                summary: Some(prose.clone()),
                ..Default::default()
            }),
            "profile",
        ),
    ] {
        let shown = says(&facts);
        assert!(shown.contains(marker), "{shown}");
        assert!(!shown.contains(&prose.text), "{shown}");
        // The source panel prints complete prose and its credit separately;
        // its metadata table must not produce another bare extract either.
        let entity = EntityRef::of(&catalog, facts.kind(), 0).unwrap();
        assert!(compared(&catalog, &entity, &facts).is_empty());
        let record = SourceRecord {
            key: entity.key,
            source: "wikipedia".into(),
            source_id: Some("Q123".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts,
        };
        let mut held = Sources::default();
        held.set(record);
        let serialized = sources::to_json(&held);
        let summary = serialized.get("records").unwrap().as_arr().unwrap()[0]
            .get("facts")
            .unwrap()
            .get("summary")
            .unwrap();
        for (field, expected) in [
            ("text", prose.text.as_str()),
            ("url", prose.url.as_str()),
            ("lang", prose.lang.as_str()),
            ("licence", prose.licence.as_str()),
        ] {
            assert_eq!(
                summary.get(field).and_then(aede_core::json::Json::as_str),
                Some(expected)
            );
        }
    }
}

#[test]
fn source_summaries_distinguish_saved_content_from_empty_completed_answers() {
    for (facts, expected, _) in source_display_cases() {
        let shown = says(&facts);
        assert!(shown.contains(expected), "{facts:?}: {shown}");
        assert!(!shown.contains("nothing"), "{facts:?}: {shown}");
    }
    for facts in empty_source_display_cases() {
        assert_eq!(says(&facts), ui::dim("nothing"), "{facts:?}");
    }
}

#[test]
fn source_panels_show_saved_images_and_credits_without_inventing_tag_verdicts() {
    let catalog = catalog_with("Jazz", "Columbia");
    for (facts, _, expected_field) in source_display_cases() {
        let entity = EntityRef::of(&catalog, facts.kind(), 0).unwrap();
        let rows = compared(&catalog, &entity, &facts);
        let (_, shown, verdict) = rows
            .iter()
            .find(|(field, _, _)| *field == expected_field)
            .unwrap_or_else(|| panic!("missing {expected_field}: {facts:?}: {rows:?}"));
        assert!(!shown.is_empty(), "{facts:?}");
        if expected_field == "genres" {
            assert!(matches!(verdict, Some(Verdict::Agrees)));
        } else {
            assert!(verdict.is_none(), "source-only metadata: {facts:?}");
        }
        if matches!(expected_field, "recording credit" | "edition credit") {
            assert!(shown.contains("Example Producer"), "{shown}");
            assert!(shown.contains("producer"), "{shown}");
        }
    }
    for facts in empty_source_display_cases() {
        let entity = EntityRef::of(&catalog, facts.kind(), 0).unwrap();
        assert!(compared(&catalog, &entity, &facts).is_empty(), "{facts:?}");
    }
}

/// Independently useful facts, including the shapes saved by manual credits,
/// successful cover downloads and successful artist/label logo downloads.
fn source_display_cases() -> Vec<(Facts, &'static str, &'static str)> {
    use aede_core::sources::{KnownRelease, Picture, TrackFacts};

    let credits = aede_core::musicbrainz::recording(
        &aede_core::json::parse(
            r#"{"id":"recording-id","relations":[{"type":"producer","target-type":"artist",
              "artist":{"id":"producer-id","name":"Example Producer"}}]}"#,
        )
        .unwrap(),
    )
    .unwrap()
    .facts
    .credits;
    let membership = aede_core::musicbrainz::artist(
        &aede_core::json::parse(
            r#"{"id":"artist-id","relations":[{"type":"member of band","direction":"backward",
              "artist":{"id":"member-id","name":"Example Member"}}]}"#,
        )
        .unwrap(),
    )
    .unwrap()
    .facts;
    let picture = Picture {
        url: "https://example.org/artwork.png".into(),
    };
    vec![
        (
            Facts::Track(TrackFacts {
                credits: credits.clone(),
                ..Default::default()
            }),
            "recording credit",
            "recording credit",
        ),
        (
            Facts::Track(TrackFacts {
                recording: Some("recording-id".into()),
                ..Default::default()
            }),
            "recording-id",
            "recording",
        ),
        (
            Facts::Release(ReleaseFacts {
                credits,
                ..Default::default()
            }),
            "edition credit",
            "edition credit",
        ),
        (
            Facts::Release(ReleaseFacts {
                cover_art: Some(picture.url.clone()),
                ..Default::default()
            }),
            "cover",
            "cover",
        ),
        (
            Facts::Release(ReleaseFacts {
                edition_mbid: Some("edition-id".into()),
                ..Default::default()
            }),
            "edition-id",
            "edition",
        ),
        (
            Facts::Release(ReleaseFacts {
                label_mbid: Some("label-id".into()),
                ..Default::default()
            }),
            "label-id",
            "label ID",
        ),
        (
            Facts::Release(ReleaseFacts {
                secondary_types: vec!["Compilation".into()],
                ..Default::default()
            }),
            "Compilation",
            "release type",
        ),
        (
            Facts::Artist(ArtistFacts {
                logo: Some(picture.clone()),
                ..Default::default()
            }),
            "logo",
            "logo",
        ),
        (
            Facts::Artist(ArtistFacts {
                country_code: Some("GB".into()),
                ..Default::default()
            }),
            "GB",
            "country code",
        ),
        (
            Facts::Artist(ArtistFacts {
                ended: Some("2001".into()),
                ..Default::default()
            }),
            "2001",
            "ended",
        ),
        (
            Facts::Artist(ArtistFacts {
                genres: vec!["Jazz".into()],
                ..Default::default()
            }),
            "Jazz",
            "genres",
        ),
        (
            Facts::Artist(ArtistFacts {
                aliases: vec!["Example Alias".into()],
                ..Default::default()
            }),
            "Example Alias",
            "also called",
        ),
        (
            Facts::Artist(ArtistFacts {
                wikidata: Some("https://www.wikidata.org/wiki/Q123".into()),
                ..Default::default()
            }),
            "Wikidata",
            "wikidata",
        ),
        (
            Facts::Artist(ArtistFacts {
                discogs: Some("https://www.discogs.com/artist/123".into()),
                ..Default::default()
            }),
            "Discogs",
            "discogs",
        ),
        (
            Facts::Artist(ArtistFacts {
                homepage: Some("https://example.org/artist".into()),
                ..Default::default()
            }),
            "homepage",
            "homepage",
        ),
        (Facts::Artist(membership), "membership", "memberships"),
        (
            Facts::Artist(ArtistFacts {
                discography: vec![KnownRelease {
                    mbid: "group-id".into(),
                    title: "Example Album".into(),
                    first_released: None,
                    primary_type: Some("Album".into()),
                    secondary_types: Vec::new(),
                }],
                ..Default::default()
            }),
            "release",
            "discography",
        ),
        (
            Facts::Label(LabelFacts {
                logo: Some(picture),
                ..Default::default()
            }),
            "logo",
            "logo",
        ),
    ]
}

fn empty_source_display_cases() -> Vec<Facts> {
    vec![
        Facts::Artist(ArtistFacts::default()),
        Facts::Release(ReleaseFacts::default()),
        Facts::Track(Default::default()),
        Facts::Label(LabelFacts::default()),
        Facts::Artist(ArtistFacts {
            discography_fetched_at: Some(1),
            ..Default::default()
        }),
        Facts::Release(ReleaseFacts {
            relationships_complete: true,
            ..Default::default()
        }),
        Facts::Track(aede_core::sources::TrackFacts {
            relationships_complete: true,
            ..Default::default()
        }),
    ]
}
