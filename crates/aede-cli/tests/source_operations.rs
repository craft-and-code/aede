//! Public regressions for M1 source decisions and portable rules.

#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[path = "fetch_output_support/mod.rs"]
mod support;

use aede_core::{json, sources, store, user};
use sources::{ReviewDecision, SourceReview, Sources};
use support::Library;

#[test]
fn reading_m1_data_creates_no_writer_lock() {
    let library = Library::new("reads", false);
    let before = filesystem::snapshot(&library.data);
    for arguments in [
        vec!["sources"],
        vec!["sources", "--list"],
        vec!["sources", "--export"],
        vec!["sources", "--template"],
        vec!["review"],
        vec!["rules"],
        vec!["fingerprint", "--list"],
    ] {
        let output = library.run(&arguments);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            filesystem::snapshot(&library.data) == before,
            "{arguments:?} must leave data files and locks untouched"
        );
    }
}

fn decisions_only(library: &Library) -> Sources {
    let mut held = sources::load_all(&sources::sources_path(&library.data))
        .unwrap()
        .unwrap();
    let record = held.records.pop().unwrap();
    held.set_review(SourceReview {
        entity: record.entity(),
        source: record.source,
        source_id: record.source_id,
        decision: ReviewDecision::Rejected,
        reviewed_at: 1,
    });
    held.exclude_credit(
        aede_core::graph::RelationRef {
            source: user::EntityRef::new(aede_core::model::EntityKind::Artist, "miles davis"),
            kind: "credit:performer".into(),
            target: user::EntityRef::new(
                aede_core::model::EntityKind::Recording,
                "73eac6f8-522a-4eab-8cf5-1e708f666e31",
            ),
            provenance: sources::MUSICBRAINZ.into(),
            source_id: None,
        },
        1,
    );
    sources::save(&held, &sources::sources_path(&library.data)).unwrap();
    held
}

#[test]
fn decisions_without_cached_records_can_be_exported_and_forgotten() {
    for selected in [false, true] {
        let library = Library::new("decisions", false);
        let held = decisions_only(&library);
        let output = library.run(&["sources", "--export"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let exported =
            sources::from_json(&json::parse(&String::from_utf8(output.stdout).unwrap()).unwrap())
                .unwrap();
        assert_eq!(exported.reviews, held.reviews);
        assert_eq!(exported.credit_exclusions, held.credit_exclusions);
        let arguments = if selected {
            vec!["sources", "--forget", "--source=musicbrainz"]
        } else {
            vec!["sources", "--forget"]
        };
        let output = library.run(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("decision"));
        let after = sources::load_all(&sources::sources_path(&library.data))
            .unwrap()
            .unwrap();
        assert!(
            after.records.is_empty()
                && after.reviews.is_empty()
                && after.credit_exclusions.is_empty()
        );
    }
}

#[test]
fn conflicting_source_operations_and_unused_filters_are_refused() {
    let library = Library::new("options", false);
    let path = sources::sources_path(&library.data);
    let before = std::fs::read(&path).unwrap();
    let before_music = filesystem::snapshot(&library.music);
    for arguments in [
        vec!["sources", "--export", "--forget"],
        vec!["sources", "--list", "--forget"],
        vec!["sources", "--source=musicbrainz"],
        vec!["sources", "--export", "--source=musicbrainz"],
    ] {
        let output = library.run(&arguments);
        assert!(!output.status.success(), "accepted {arguments:?}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    assert_eq!(filesystem::snapshot(&library.music), before_music);
}

#[test]
fn an_empty_review_still_checks_its_window_options() {
    let library = Library::new("review", false);
    for arguments in [
        vec!["review", "--limit=bad"],
        vec!["review", "--offset=bad"],
    ] {
        let output = library.run(&arguments);
        assert!(!output.status.success(), "accepted {arguments:?}");
    }
}

#[test]
fn fingerprint_listing_escapes_controls_without_changing_the_catalog() {
    let library = Library::new("fingerprint", false);
    let path = store::catalog_path(&library.data);
    let mut catalog = store::load(&path).unwrap().unwrap();
    catalog.files[0].path.push_str("\u{1b}[2J\rhidden");
    catalog.files[0]
        .fingerprint
        .as_mut()
        .unwrap()
        .data
        .push_str("\u{1b}]0;title\u{7}");
    store::save(&catalog, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let before_music = filesystem::snapshot(&library.music);
    let before_audio = std::fs::read(&library.audio).unwrap();
    let output = library.run(&["fingerprint", "--list"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.contains(&0x1b)
            && !output.stdout.contains(&b'\r')
            && !output.stdout.contains(&7)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("\\u{1b}"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(filesystem::snapshot(&library.music), before_music);
    assert_eq!(std::fs::read(&library.audio).unwrap(), before_audio);
}

#[cfg(unix)]
#[test]
fn rules_import_checks_both_destinations_before_writing_either() {
    let library = Library::new("rules", false);
    let user_path = user::user_path(&library.data);
    let mut incoming = user::UserData::default();
    incoming.same_artist.push(user::SameArtist {
        owner: user::LOCAL_USER.into(),
        spelling: "miles dewey davis".into(),
        filed_as: "miles davis".into(),
        created_at: 1,
    });
    user::save(&incoming, &user_path).unwrap();
    let rule_path = library.directory.join("rules.json");
    let export = library.run(&[
        "rules",
        "--export",
        &format!("--output={}", rule_path.display()),
    ]);
    assert!(export.status.success());
    user::save(&user::UserData::default(), &user_path).unwrap();
    let before_user = std::fs::read(&user_path).unwrap();
    let source_path = sources::sources_path(&library.data);
    let real = library.directory.join("real-sources.json");
    std::fs::rename(&source_path, &real).unwrap();
    std::os::unix::fs::symlink(&real, &source_path).unwrap();
    let before_sources = std::fs::read(&real).unwrap();
    let output = library.run(&["rules", &format!("--import={}", rule_path.display())]);
    assert!(!output.status.success());
    assert_eq!(std::fs::read(&user_path).unwrap(), before_user);
    assert_eq!(std::fs::read(&real).unwrap(), before_sources);

    let fresh = library.directory.join("new-data");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_aede"))
        .arg("rules")
        .arg(format!("--import={}", rule_path.display()))
        .arg(format!("--data={}", fresh.display()))
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        user::load(&user::user_path(&fresh))
            .unwrap()
            .unwrap()
            .same_artist
            .len(),
        1
    );
}

#[test]
fn a_rules_bundle_cannot_import_nonportable_data() {
    let library = Library::new("rules-scope", false);
    let rule_path = library.directory.join("rules.json");
    let export = library.run(&[
        "rules",
        "--export",
        &format!("--output={}", rule_path.display()),
    ]);
    assert!(export.status.success());
    let source_path = sources::sources_path(&library.data);
    let before_sources = std::fs::read(&source_path).unwrap();
    let original = json::parse(&std::fs::read_to_string(&rule_path).unwrap()).unwrap();
    let mut unrelated = user::UserData::default();
    unrelated
        .entry(
            user::LOCAL_USER,
            &user::EntityRef::new(aede_core::model::EntityKind::Artist, "miles davis"),
            1,
        )
        .loved = true;
    for (table, value) in [
        (
            "sources",
            json::parse(&String::from_utf8(before_sources.clone()).unwrap()).unwrap(),
        ),
        ("user", user::to_json(&unrelated)),
    ] {
        let mut document = original.clone();
        document.set(table, value);
        std::fs::write(&rule_path, document.to_string_pretty()).unwrap();
        let output = library.run(&["rules", &format!("--import={}", rule_path.display())]);
        assert!(
            !output.status.success(),
            "{table} data outside personal rules was accepted"
        );
        assert!(!user::user_path(&library.data).exists());
        assert_eq!(std::fs::read(&source_path).unwrap(), before_sources);
    }
}

#[test]
fn source_paragraphs_and_interactive_review_show_imported_text_literally() {
    let library = Library::new("paragraphs", false);
    let path = sources::sources_path(&library.data);
    let mut held = sources::load_all(&path).unwrap().unwrap();
    let record = &mut held.records[0];
    record.source = "manual".into();
    record.confidence = sources::Confidence::Matched(80);
    if let sources::Facts::Artist(facts) = &mut record.facts {
        facts.kind = Some("test\u{1b}[2J\rhidden".into());
        facts.summary = Some(sources::Prose {
            text: "Imported test paragraph\u{1b}[2J\rhidden".into(),
            url: "https://example.org/imported\u{1b}]0;title\u{7}".into(),
            lang: "en".into(),
            licence: "test data".into(),
        });
    }
    sources::save(&held, &path).unwrap();
    let before = std::fs::read(&path).unwrap();
    for arguments in [
        vec!["artist", "Miles Davis"],
        vec!["review", "--interactive"],
    ] {
        let output = library.run(&arguments);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !output.stdout.contains(&0x1b)
                && !output.stdout.contains(&b'\r')
                && !output.stdout.contains(&7),
            "unsafe {arguments:?}"
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("\\u{1b}"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

#[test]
fn credit_notes_remain_visible_when_an_exact_source_artist_enters_local_tags() {
    use aede_core::model::{self, EntityKind, ScannedFile};
    use aede_core::sources::{Confidence, CreditLink, Facts, SourceRecord, TrackFacts};
    use aede_core::tags::RawTags;

    let library = Library::new("credit-note-identity", false);
    let fixture = include_bytes!("../../aede-core/tests/fixtures/track.flac");
    std::fs::write(&library.audio, fixture).unwrap();
    let extra = library.music.join("contributor").join("01.flac");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(&extra, fixture).unwrap();
    let build = |local: bool| {
        let files = [
            ("Local Player", "player-id", &library.audio),
            ("External Writer", "writer-id", &extra),
        ]
        .into_iter()
        .take(if local { 2 } else { 1 })
        .enumerate()
        .map(|(index, (artist, mbid, path))| {
            let mut tags = RawTags::default();
            for (key, value) in [
                ("artist", artist),
                ("albumartist", artist),
                ("musicbrainz_artistid", mbid),
                ("album", "An Album"),
            ] {
                tags.insert(key, value);
            }
            tags.insert("title", format!("Song {index}"));
            tags.insert("musicbrainz_recordingid", format!("recording-{index}"));
            ScannedFile {
                path: path.to_string_lossy().into_owned(),
                size: fixture.len() as u64,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
        model::build(
            files,
            vec![library.music.to_string_lossy().into_owned()],
            1,
            &[],
        )
    };
    let before = build(false);
    let after = build(true);
    let path = store::catalog_path(&library.data);
    store::save(&before, &path).unwrap();
    let held = Sources {
        records: vec![SourceRecord {
            key: library.audio.to_string_lossy().into_owned(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("recording-0".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Track(TrackFacts {
                credits: vec![CreditLink {
                    relation_id: Some("same-credit-row".into()),
                    role_id: None,
                    role: "composer".into(),
                    direction: None,
                    artist_mbid: "writer-id".into(),
                    artist_name: "External Writer".into(),
                    credited_as: None,
                    attributes: Vec::new(),
                    began: None,
                    ended: None,
                    over: None,
                    order: None,
                }],
                relationships_complete: true,
                ..Default::default()
            }),
        }],
        ..Default::default()
    };
    sources::save(&held, &sources::sources_path(&library.data)).unwrap();
    user::save(&user::UserData::default(), &user::user_path(&library.data)).unwrap();
    aede_core::conclusions::save(
        &Default::default(),
        &aede_core::conclusions::conclusions_path(&library.data),
    )
    .unwrap();
    let source_before = std::fs::read(sources::sources_path(&library.data)).unwrap();
    let audio_before = filesystem::snapshot(&library.music);
    let read = |arguments: &[&str]| {
        let before = filesystem::snapshot(&library.data);
        let output = library.run(arguments);
        assert!(output.status.success(), "{arguments:?}: {output:?}");
        assert_eq!(
            filesystem::snapshot(&library.data),
            before,
            "{arguments:?} changed a store"
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let edge = aede_core::graph::edges(&before, &held)
        .into_iter()
        .find(|edge| edge.reference.kind == "credit:composer")
        .unwrap();
    assert_eq!(
        edge.reference.source,
        user::EntityRef::new(EntityKind::Artist, "mbid:writer-id")
    );
    let output = library.run(&[
        "relation",
        &edge.reference.id(),
        "--text=Retained musical context",
        "--tag=booklet",
    ]);
    assert!(output.status.success(), "{output:?}");
    let personal_before = std::fs::read(user::user_path(&library.data)).unwrap();
    let mut missing = Vec::new();
    for (stage, catalog) in [
        ("source only", &before),
        ("exact artist local", &after),
        ("source only again", &before),
    ] {
        store::save(catalog, &path).unwrap();
        let rows = json::parse(&read(&["relations", "--tag=booklet", "--json", "--all"])).unwrap();
        if rows.as_arr().unwrap().len() != 1 {
            missing.push(stage);
        } else {
            assert_eq!(
                rows.as_arr().unwrap()[0]
                    .get("annotation")
                    .unwrap()
                    .field_str("note")
                    .as_deref(),
                Some("Retained musical context")
            );
        }
        let edge = aede_core::graph::edges(catalog, &held)
            .into_iter()
            .find(|edge| edge.reference.kind == "credit:composer")
            .unwrap();
        let detail = read(&["relation", &edge.reference.id()]);
        if !detail.contains("Retained musical context") {
            missing.push(stage);
        }
        let personal = user::load(&user::user_path(&library.data))
            .unwrap()
            .unwrap();
        assert!(
            aede_core::doctor::orphaned_relation_annotations(catalog, &held, &personal).is_empty(),
            "the same live credit is not an orphan: {stage}"
        );
        let report = library.directory.join("graph.json");
        read(&["export", "--graph", "--output", report.to_str().unwrap()]);
        let graph = json::parse(&std::fs::read_to_string(&report).unwrap()).unwrap();
        let credit = graph
            .get("relations")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .find(|row| row.field_str("relation").as_deref() == Some("credit:composer"))
            .unwrap();
        if credit
            .get("annotation")
            .unwrap()
            .field_str("note")
            .as_deref()
            != Some("Retained musical context")
        {
            missing.push(stage);
        }
        assert_eq!(
            std::fs::read(user::user_path(&library.data)).unwrap(),
            personal_before
        );
        assert_eq!(
            std::fs::read(sources::sources_path(&library.data)).unwrap(),
            source_before
        );
    }
    assert_eq!(filesystem::snapshot(&library.music), audio_before);
    assert!(
        missing.is_empty(),
        "the unchanged credit lost its personal context: {missing:?}"
    );
    store::save(&after, &path).unwrap();
    let local_edge = aede_core::graph::edges(&after, &held)
        .into_iter()
        .find(|edge| edge.reference.kind == "credit:composer")
        .unwrap();
    let output = library.run(&[
        "relation",
        &local_edge.reference.id(),
        "--text=Edited through local identity",
    ]);
    assert!(output.status.success(), "{output:?}");
    let personal = user::load(&user::user_path(&library.data))
        .unwrap()
        .unwrap();
    assert_eq!(personal.relation_annotations.len(), 1);
    assert_eq!(personal.relation_annotations[0].relation, edge.reference);
    assert_eq!(
        personal.relation_annotations[0].note.as_deref(),
        Some("Edited through local identity")
    );
    let output = library.run(&[
        "relation",
        &local_edge.reference.id(),
        "--tag=booklet",
        "--remove",
    ]);
    assert!(output.status.success(), "{output:?}");
    let personal = user::load(&user::user_path(&library.data))
        .unwrap()
        .unwrap();
    assert_eq!(personal.relation_annotations.len(), 1);
    assert!(personal.relation_annotations[0].tags.is_empty());
    assert_eq!(
        personal.relation_annotations[0].note.as_deref(),
        Some("Edited through local identity")
    );
    let output = library.run(&["relation", &local_edge.reference.id(), "--remove"]);
    assert!(output.status.success(), "{output:?}");
    assert!(
        user::load(&user::user_path(&library.data))
            .unwrap()
            .unwrap()
            .relation_annotations
            .is_empty()
    );
    let output = library.run(&[
        "relation",
        &local_edge.reference.id(),
        "--text=New local credit context",
    ]);
    assert!(output.status.success(), "{output:?}");
    let personal = user::load(&user::user_path(&library.data))
        .unwrap()
        .unwrap();
    assert_eq!(
        personal.relation_annotations[0].relation.source,
        edge.reference.source
    );
    store::save(&before, &path).unwrap();
    assert!(read(&["relation", &edge.reference.id()]).contains("New local credit context"));
    assert_eq!(filesystem::snapshot(&library.music), audio_before);
    assert_eq!(
        std::fs::read(sources::sources_path(&library.data)).unwrap(),
        source_before
    );
}

#[test]
fn track_parent_navigation_keeps_the_trusted_snapshot_and_independent_evidence() {
    use aede_core::model::{self, CreditAttribute, ScannedFile};
    use aede_core::sources::{
        Confidence, Facts, SourceRecord, TrackFacts, WorkLink, WorkParentLink,
    };
    use aede_core::tags::RawTags;

    const RECORDING: &str = "73eac6f8-522a-4eab-8cf5-1e708f666e31";
    for scenario in [
        "pending-first",
        "pending-reversed",
        "rejected-first",
        "accepted-older-lookup",
        "latest-trusted-first",
        "latest-trusted-reversed",
        "both-pending",
        "both-rejected",
        "different-sources",
        "different-parents",
    ] {
        let library = Library::new("parent-snapshots", false);
        let previous = store::load(&store::catalog_path(&library.data))
            .unwrap()
            .unwrap();
        let second_audio = library.music.join("02.flac");
        std::fs::write(&second_audio, b"another unchanged audio sentinel").unwrap();
        let files = [&library.audio, &second_audio]
            .into_iter()
            .enumerate()
            .map(|(index, audio)| {
                let mut tags = RawTags {
                    fields: previous.files[0].tags.clone(),
                    ..Default::default()
                };
                tags.fields.remove("title");
                tags.insert(
                    "title",
                    if index == 0 {
                        "So What"
                    } else {
                        "Another Placement"
                    },
                );
                ScannedFile {
                    path: audio.to_string_lossy().into_owned(),
                    size: 24,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }
            })
            .collect();
        let catalog = model::build(files, previous.roots, 1, &[]);
        assert_eq!(catalog.recordings.len(), 1);
        store::save(&catalog, &store::catalog_path(&library.data)).unwrap();
        let mut records: Vec<_> = [&library.audio, &second_audio]
            .into_iter()
            .enumerate()
            .map(|(index, audio)| SourceRecord {
                key: audio.to_string_lossy().into_owned(),
                source: sources::MUSICBRAINZ.into(),
                source_id: Some(RECORDING.into()),
                confidence: if index == 0 {
                    Confidence::matched(75)
                } else {
                    Confidence::Identified
                },
                fetched_at: if index == 0 { 20 } else { 10 },
                facts: Facts::Track(TrackFacts {
                    recording: Some(RECORDING.into()),
                    works: vec![WorkLink {
                        mbid: "child-work".into(),
                        title: "Child Work".into(),
                        parents: vec![WorkParentLink {
                            mbid: "parent-work".into(),
                            title: if index == 0 {
                                "Old Parent"
                            } else {
                                "New Parent"
                            }
                            .into(),
                            relation_id: Some("same-parent-relationship".into()),
                            attributes: vec![CreditAttribute {
                                id: None,
                                name: if index == 0 { "movement" } else { "act" }.into(),
                                value: None,
                                credited_as: None,
                            }],
                            order: Some(index as u32 + 1),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            })
            .collect();
        let mut decisions = [None, None];
        match scenario {
            "rejected-first" => {
                records[0].confidence = Confidence::Identified;
                decisions[0] = Some(ReviewDecision::Rejected);
            }
            "accepted-older-lookup" => decisions[0] = Some(ReviewDecision::Accepted),
            "latest-trusted-first" | "latest-trusted-reversed" => {
                records[0].confidence = Confidence::Identified;
                records[0].fetched_at = 10;
                records[1].fetched_at = 20;
            }
            "both-pending" => records[1].confidence = Confidence::matched(80),
            "both-rejected" => decisions = [Some(ReviewDecision::Rejected); 2],
            "different-sources" => records[1].source = "second-opinion".into(),
            "different-parents" => {
                let Facts::Track(facts) = &mut records[1].facts else {
                    unreachable!();
                };
                facts.works[0].parents[0].mbid = "another-parent".into();
            }
            _ => {}
        }
        let mut held = Sources::default();
        for (record, decision) in records.iter().zip(decisions) {
            if let Some(decision) = decision {
                held.set_review(SourceReview {
                    entity: record.entity(),
                    source: record.source.clone(),
                    source_id: record.source_id.clone(),
                    decision,
                    reviewed_at: 30,
                });
            }
        }
        if scenario.ends_with("reversed") {
            records.reverse();
        }
        held.records = records;
        sources::save(&held, &sources::sources_path(&library.data)).unwrap();
        user::save(&user::UserData::default(), &user::user_path(&library.data)).unwrap();
        aede_core::conclusions::save(
            &Default::default(),
            &aede_core::conclusions::conclusions_path(&library.data),
        )
        .unwrap();
        let stores_before = filesystem::snapshot(&library.data);
        let audio_before = filesystem::snapshot(&library.music);
        let read = |arguments: &[&str]| {
            let output = library.run(arguments);
            assert!(
                output.status.success(),
                "{scenario}: {arguments:?}: {output:?}"
            );
            assert_eq!(
                filesystem::snapshot(&library.data),
                stores_before,
                "{scenario}: {arguments:?}"
            );
            assert_eq!(
                filesystem::snapshot(&library.music),
                audio_before,
                "{scenario}: {arguments:?}"
            );
            String::from_utf8(output.stdout).unwrap()
        };
        let page = read(&["track", "So What"]);
        let trusted = !matches!(scenario, "both-pending" | "both-rejected");
        let parent = if scenario == "different-parents" {
            "another-parent"
        } else {
            "parent-work"
        };
        assert_eq!(
            page.contains(&format!("aede work '{parent}'")),
            trusted,
            "{scenario} lost the parent relationship's navigation: {page}"
        );
        let separate = matches!(scenario, "different-sources" | "different-parents");
        let expected_title = if matches!(
            scenario,
            "accepted-older-lookup" | "both-pending" | "both-rejected"
        ) {
            "Old Parent"
        } else {
            "New Parent"
        };
        assert!(page.contains(expected_title), "{scenario}: {page}");
        if !separate {
            let hidden_title = if expected_title == "Old Parent" {
                "New Parent"
            } else {
                "Old Parent"
            };
            assert!(!page.contains(hidden_title), "{scenario}: {page}");
        } else {
            assert!(
                page.contains("Old Parent") && page.contains("New Parent"),
                "{scenario}: {page}"
            );
        }
        let track = json::parse(&read(&["track", "So What", "--json"])).unwrap();
        assert_eq!(
            track.as_arr().unwrap()[0]
                .get("parent_works")
                .unwrap()
                .as_arr()
                .unwrap()
                .len(),
            2
        );
        let query = json::parse(&read(&["query", &format!("work:{parent}"), "--json"])).unwrap();
        assert_eq!(
            query.as_arr().unwrap().len(),
            if trusted { 2 } else { 0 },
            "{scenario}"
        );
        let exported = json::parse(&read(&["export", "--graph"])).unwrap();
        assert_eq!(
            sources::from_json(exported.get("sources").unwrap()).unwrap(),
            held
        );
        let parents: Vec<_> = exported
            .get("relations")
            .unwrap()
            .as_arr()
            .unwrap()
            .iter()
            .filter(|row| row.field_str("relation").as_deref() == Some("part_of_work"))
            .collect();
        assert_eq!(parents.len(), if separate { 2 } else { 1 });
        if !separate {
            assert_eq!(
                parents[0].field_str("target_name").as_deref(),
                Some(expected_title)
            );
            assert_eq!(
                parents[0].get("trusted").and_then(json::Json::as_bool),
                Some(trusted)
            );
            assert_eq!(parents[0].field_u64("weight"), Some(2));
        }
    }
}
