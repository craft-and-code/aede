//! Graph and recording detail is literal in terminals and exact in exports.

#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[path = "fetch_output_support/mod.rs"]
mod support;

use aede_core::{conclusions, graph, json, model, sources, store, tags::RawTags, user};
use sources::{Confidence, CreditLink, Facts, SourceRecord, Sources, TrackFacts};
use support::Library;

#[test]
fn graph_details_escape_source_and_personal_controls_without_changing_evidence() {
    const CONTROLS: &str = "\u{1b}]8;;https://example.invalid\u{7}\r\u{85}end";
    for controls in ["", CONTROLS] {
        let library = Library::new("relation-terminal", false);
        #[cfg(unix)]
        let library = {
            let mut library = library;
            if !controls.is_empty() {
                let moved = library.directory.join("data\u{1b}[2J\u{7}");
                std::fs::rename(&library.data, &moved).unwrap();
                library.data = moved;
            }
            library
        };
        let fixture = include_bytes!("../../aede-core/tests/fixtures/track.flac");
        std::fs::write(&library.audio, fixture).unwrap();
        let value = |name: &str| format!("{name}{controls}");
        let original = store::load(&store::catalog_path(&library.data))
            .unwrap()
            .unwrap();
        let mut tags = RawTags {
            fields: original.files[0].tags.clone(),
            ..Default::default()
        };
        tags.fields.remove("title");
        tags.insert("title", value("Example Recording"));
        for field in ["artist", "albumartist"] {
            tags.fields.remove(field);
            tags.insert(
                field,
                if controls.is_empty() {
                    "Example Band"
                } else {
                    "Example Band\u{1b}[2J\u{7}\r\u{85}end"
                },
            );
        }
        tags.insert("date", "2000");
        tags.insert("work", value("Canonical Work"));
        tags.insert("musicbrainz_workid", "canonical-work");
        let catalog = model::build(
            vec![model::ScannedFile {
                path: library.audio.to_string_lossy().into_owned(),
                size: fixture.len() as u64,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }],
            original.roots,
            1,
            &[],
        );
        store::save(&catalog, &store::catalog_path(&library.data)).unwrap();
        let mut held = Sources {
            records: vec![SourceRecord {
                key: library.audio.to_string_lossy().into_owned(),
                source: value("example-provider"),
                source_id: catalog.recordings[0].mbid.clone(),
                fetched_at: 1,
                confidence: Confidence::Identified,
                facts: Facts::Track(TrackFacts {
                    recording: catalog.recordings[0].mbid.clone(),
                    credits: vec![CreditLink {
                        relation_id: Some(value("relationship-id")),
                        role_id: Some(value("relationship-type")),
                        role: value("producer"),
                        direction: Some(value("backward")),
                        artist_mbid: "example-producer".into(),
                        artist_name: value("Example Producer"),
                        credited_as: Some(value("Exact Credited Name")),
                        attributes: vec![model::CreditAttribute {
                            id: Some("instrument-id".into()),
                            name: value("Instrument"),
                            value: Some(value("Electric")),
                            credited_as: Some(value("Played As")),
                        }],
                        began: Some(value("2000")),
                        ended: Some(value("2001")),
                        over: Some(true),
                        order: Some(2),
                    }],
                    works: vec![sources::WorkLink {
                        mbid: value("sourced-work"),
                        title: value("Sourced Work"),
                        attributes: vec![model::CreditAttribute {
                            id: None,
                            name: value("Source Work Qualifier"),
                            value: None,
                            credited_as: None,
                        }],
                        ..Default::default()
                    }],
                    relationships_complete: true,
                    ..Default::default()
                }),
            }],
            ..Default::default()
        };
        let artist = catalog
            .artist(catalog.releases[0].album_artist_id.unwrap())
            .unwrap();
        held.records.push(SourceRecord {
            key: artist.key.clone(),
            source: sources::MUSICBRAINZ.into(),
            source_id: artist.mbid.clone(),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(sources::ArtistFacts {
                discography: vec![sources::KnownRelease {
                    mbid: "missing-album".into(),
                    title: value("Missing Album"),
                    first_released: Some("2001".into()),
                    primary_type: Some("Album".into()),
                    secondary_types: vec![],
                }],
                members: vec![sources::Membership {
                    relation_id: Some("membership-id".into()),
                    role_id: None,
                    direction: Some("backward".into()),
                    mbid: "current-player".into(),
                    name: value("Current Player"),
                    credited_as: None,
                    side: sources::Side::Player,
                    kind: "member of band".into(),
                    attributes: vec!["guitar".into()],
                    began: Some("1990".into()),
                    ended: None,
                    over: Some(false),
                }],
                ..Default::default()
            }),
        });
        sources::save(&held, &sources::sources_path(&library.data)).unwrap();
        let edge = graph::edges(&catalog, &held)
            .into_iter()
            .find(|edge| edge.reference.kind == format!("credit:{}", value("producer")))
            .unwrap();
        let mut personal = user::UserData::default();
        let annotation = personal
            .relation_entry_in(&catalog, user::LOCAL_USER, &edge.reference, 1)
            .unwrap();
        annotation.note = Some(value("Personal Note"));
        annotation.tags.insert(value("Personal Tag"));
        user::save(&personal, &user::user_path(&library.data)).unwrap();
        conclusions::save(
            &conclusions::Conclusions::default(),
            &conclusions::conclusions_path(&library.data),
        )
        .unwrap();
        // Initialize the fixture lock before taking filesystem snapshots.
        std::fs::write(library.data.join(".aede.lock"), b"").unwrap();
        assert!(
            library
                .run(&["relations", "--all", "--json"])
                .status
                .success()
        );
        let data_before = filesystem::snapshot(&library.data);
        let music_before = filesystem::snapshot(&library.music);
        let read = |arguments: &[&str]| {
            let output = library.run(arguments);
            assert!(output.status.success(), "{arguments:?}: {output:?}");
            assert_eq!(filesystem::snapshot(&library.data), data_before);
            assert_eq!(filesystem::snapshot(&library.music), music_before);
            String::from_utf8(output.stdout).unwrap()
        };
        let id = edge.reference.id();
        let album = read(&["album", "relation-terminal"]);
        assert!(
            !album
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')),
            "terminal instructions leaked from album: {album:?}"
        );
        let member = if controls.is_empty() {
            "Current Player".to_string()
        } else {
            "Current Player\\u{1b}]8;;https://example.invalid\\u{7}\\r\\u{85}end".to_string()
        };
        assert!(
            album.contains(&format!("line-up in 2000: {member}")),
            "{album:?}"
        );
        let recording = read(&["recording", catalog.recordings[0].mbid.as_deref().unwrap()]);
        assert!(
            !recording
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')),
            "terminal instructions leaked from recording: {recording:?}"
        );
        for marker in [
            "Canonical Work",
            "Sourced Work",
            "sourced-work",
            "Source Work Qualifier",
            "example-provider",
        ] {
            let expected = if controls.is_empty() {
                marker.to_string()
            } else {
                format!("{marker}\\u{{1b}}]8;;https://example.invalid\\u{{7}}\\r\\u{{85}}end")
            };
            assert!(
                recording.contains(&expected),
                "missing {expected:?}: {recording:?}"
            );
        }
        let detail = read(&["relation", &id]);
        assert!(
            !detail
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')),
            "terminal instructions leaked: {detail:?}"
        );
        for marker in [
            "Example Recording",
            "Example Producer",
            "example-provider",
            "relationship-id",
            "relationship-type",
            "producer",
            "backward",
            "Exact Credited Name",
            "Instrument",
            "Electric",
            "Played As",
            "2000",
            "2001",
            "Personal Tag",
            "Personal Note",
        ] {
            let expected = if controls.is_empty() {
                marker.to_string()
            } else {
                format!("{marker}\\u{{1b}}]8;;https://example.invalid\\u{{7}}\\r\\u{{85}}end")
            };
            assert!(
                detail.contains(&expected),
                "missing {expected:?}: {detail:?}"
            );
        }
        let rows = json::parse(&read(&["relations", "--all", "--json"])).unwrap();
        let json::Json::Arr(rows) = rows else {
            panic!("relations must export a JSON array")
        };
        let exported = rows
            .iter()
            .find(|row| row.field_str("id").as_deref() == Some(id.as_str()))
            .unwrap();
        assert_eq!(
            exported.field_str("source_name"),
            Some(value("Example Producer"))
        );
        assert_eq!(
            exported.field_str("target_name"),
            Some(value("Example Recording"))
        );
        assert_eq!(
            exported.field_str("provenance"),
            Some(value("example-provider"))
        );
        assert_eq!(
            exported.field_str("credited_as"),
            Some(value("Exact Credited Name"))
        );
        assert_eq!(
            exported.field_str("source_id"),
            Some(value("relationship-id"))
        );
        assert_eq!(
            exported.field_str("relationship_type_id"),
            Some(value("relationship-type"))
        );
        assert_eq!(exported.field_str("direction"), Some(value("backward")));
        assert_eq!(exported.field_str("began"), Some(value("2000")));
        assert_eq!(exported.field_str("ended"), Some(value("2001")));
        let annotation = exported.get("annotation").unwrap();
        assert_eq!(annotation.field_str("note"), Some(value("Personal Note")));
        assert_eq!(
            annotation.get("tags").unwrap().as_arr().unwrap(),
            [json::Json::Str(value("Personal Tag"))]
        );
        let raw = read(&["sources", "--export"]);
        assert_eq!(
            sources::from_json(&json::parse(&raw).unwrap()).unwrap(),
            held
        );
        let saved = user::load(&user::user_path(&library.data))
            .unwrap()
            .unwrap();
        assert_eq!(user::to_json(&saved), user::to_json(&personal));

        // The neighboring save-path status already uses the shared formatter.
        let output = library.run(&["relation", &id, "--text=Replacement Note"]);
        assert!(output.status.success(), "{output:?}");
        let written = String::from_utf8(output.stdout).unwrap();
        assert!(
            !written
                .chars()
                .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')),
            "terminal instructions leaked during annotation: {written:?}"
        );
        assert_eq!(filesystem::snapshot(&library.music), music_before);
        assert_eq!(
            sources::load_all(&sources::sources_path(&library.data))
                .unwrap()
                .unwrap(),
            held
        );
        assert_eq!(
            user::load(&user::user_path(&library.data))
                .unwrap()
                .unwrap()
                .find_relation_in(&catalog, user::LOCAL_USER, &edge.reference)
                .unwrap()
                .note
                .as_deref(),
            Some("Replacement Note")
        );
        let unchanged = ["catalog.json", "sources.json", "conclusions.json"]
            .map(|name| std::fs::read(library.data.join(name)).unwrap());
        for remove in [false, true] {
            let mut arguments = vec!["missing", "--forget", "Missing Album"];
            if remove {
                arguments.push("--remove");
            }
            let output = library.run(&arguments);
            assert!(output.status.success(), "{arguments:?}: {output:?}");
            let message = String::from_utf8(output.stdout).unwrap();
            assert!(
                !message
                    .chars()
                    .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\t')),
                "terminal instructions leaked from missing confirmation: {message:?}"
            );
            let title = if controls.is_empty() {
                "Missing Album".to_string()
            } else {
                "Missing Album\\u{1b}]8;;https://example.invalid\\u{7}\\r\\u{85}end".to_string()
            };
            assert!(message.contains(&title), "{message:?}");
            if !remove {
                let artist = if controls.is_empty() {
                    "Example Band".to_string()
                } else {
                    "Example Band\\u{1b}[2J\\u{7}\\r\\u{85}end".to_string()
                };
                assert!(message.contains(&artist), "{message:?}");
            }
            assert_eq!(
                ["catalog.json", "sources.json", "conclusions.json"]
                    .map(|name| std::fs::read(library.data.join(name)).unwrap()),
                unchanged
            );
            assert_eq!(filesystem::snapshot(&library.music), music_before);
            let saved = user::load(&user::user_path(&library.data))
                .unwrap()
                .unwrap();
            assert_eq!(saved.set_aside.len(), usize::from(!remove));
            if !remove {
                assert_eq!(saved.set_aside[0].title, value("Missing Album"));
            }
            assert_eq!(
                saved
                    .find_relation_in(&catalog, user::LOCAL_USER, &edge.reference)
                    .unwrap()
                    .note
                    .as_deref(),
                Some("Replacement Note")
            );
        }
    }
}
