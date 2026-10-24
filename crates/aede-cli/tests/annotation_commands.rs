//! Less common personal commands keep their output and mutation contracts.

#[path = "annotation_command_support/mod.rs"]
mod support;

use aede_core::json::{self, Json};
use aede_core::model::EntityKind;
use aede_core::user::{EntityRef, LOCAL_USER};
use support::PersonalLibrary;

#[test]
fn doctor_reports_only_the_local_owners_waiting_relation_annotations() {
    let library = PersonalLibrary::new();
    let local = aede_core::graph::RelationRef {
        source: EntityRef::new(EntityKind::Track, "/missing/local-only.flac"),
        kind: "composer".into(),
        target: EntityRef::new(EntityKind::Artist, "local-only"),
        provenance: "manual".into(),
        source_id: None,
    };
    let foreign = aede_core::graph::RelationRef {
        source: EntityRef::new(EntityKind::Track, "/missing/foreign-only.flac"),
        target: EntityRef::new(EntityKind::Artist, "foreign-only"),
        ..local.clone()
    };
    let live = aede_core::graph::edges(&library.catalog, &Default::default())
        .into_iter()
        .next()
        .expect("the fixture has a main-artist credit")
        .reference;
    let snapshots = || {
        [
            "catalog.json",
            "conclusions.json",
            "sources.json",
            "user.json",
        ]
        .map(|name| std::fs::read(library.dir.join(name)).ok())
    };
    for (own, other) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut data = library.data();
        data.relation_annotations.clear();
        data.relation_entry(LOCAL_USER, &live, 1).note = Some("a live annotation".into());
        data.relation_entry("another-owner", &live, 1).note = Some("another live note".into());
        if own {
            data.relation_entry(LOCAL_USER, &local, 1).note = Some("keep my waiting note".into());
        }
        if other {
            data.relation_entry("another-owner", &foreign, 1).note =
                Some("keep another owner's waiting note".into());
        }
        library.save(&data);
        let before = snapshots();
        for machine in [false, true] {
            let args = if machine {
                vec!["doctor", "--all", "--json"]
            } else {
                vec!["doctor", "--all"]
            };
            let output = library.run(&args);
            assert!(output.status.success(), "{output:?}");
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(!text.contains("foreign-only"), "{text}");
            assert_eq!(text.contains("local-only"), own, "{text}");
            if machine {
                let value = json::parse(&text).unwrap();
                let waiting = value
                    .as_arr()
                    .unwrap()
                    .iter()
                    .filter(|issue| {
                        issue.field_str("type").as_deref() == Some("relation annotation is waiting")
                    })
                    .count();
                assert_eq!(waiting, usize::from(own), "{text}");
            }
            assert_eq!(snapshots(), before, "doctor changed stores for {args:?}");
        }
    }
}

#[test]
fn flag_values_are_refused_before_confirmation_help_or_personal_mutation() {
    let library = PersonalLibrary::new();
    assert!(library.run(&["played", "Song"]).status.success());
    let before = library.bytes();
    let catalog_path = aede_core::store::catalog_path(&library.dir);
    let catalog_before = std::fs::read(&catalog_path).unwrap();
    for args in [
        vec!["history", "--remove", "--yes=false"],
        vec!["history", "--remove", "--yes=true"],
        vec!["history", "--remove", "--yes="],
        vec!["history", "--remove=false", "--yes"],
        vec!["history", "--help=false"],
        vec!["--help=false"],
        vec!["--version=false"],
        vec!["stats", "--json=false"],
        vec!["stats", "-j=false"],
        vec!["stats", "--no-color=false"],
    ] {
        let result = library.run(&args);
        assert_eq!(result.status.code(), Some(2), "accepted {args:?}");
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains("does not accept a value"), "{error}");
        assert_eq!(library.bytes(), before, "changed personal data: {args:?}");
        assert_eq!(std::fs::read(&catalog_path).unwrap(), catalog_before);
    }
    for args in [vec!["--help"], vec!["stats", "--json", "--limit=1"]] {
        assert!(library.run(&args).status.success(), "refused {args:?}");
        assert_eq!(library.bytes(), before);
    }
    assert!(
        library
            .run(&["history", "--remove", "--yes"])
            .status
            .success()
    );
    let remaining = library.data();
    assert!(remaining.plays.is_empty());
    assert_eq!(
        remaining
            .find(LOCAL_USER, &library.track)
            .unwrap()
            .note
            .as_deref(),
        Some("preserve this note")
    );
}

#[test]
fn an_empty_favourites_export_keeps_its_shape_and_excludes_other_owners() {
    let library = PersonalLibrary::new();
    let mut data = library.data();
    data.entry("other", &library.track, 1).loved = true;
    library.save(&data);
    let before = library.bytes();
    for command in ["favourites", "favorites"] {
        let result = library.run(&[command, "--json"]);
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        assert!(
            matches!(json::parse(&text), Ok(Json::Arr(rows)) if rows.is_empty()),
            "{text}"
        );
        let result = library.run(&[command, "--csv"]);
        assert!(result.status.success());
        let text = String::from_utf8(result.stdout).unwrap();
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.starts_with("kind,name,reference,"), "{text}");
    }
    let path = library.dir.join("empty.json");
    let result = library.run(&["favourites", "--json", "--output", path.to_str().unwrap()]);
    assert!(result.status.success());
    assert!(
        matches!(json::parse(&std::fs::read_to_string(path).unwrap()), Ok(Json::Arr(rows)) if rows.is_empty())
    );
    assert_eq!(library.bytes(), before);
    let invalid_window = library.run(&["favourites", "--limit=0"]);
    assert!(!invalid_window.status.success());
}

#[test]
fn manual_listens_refuse_non_track_references_before_changing_the_store() {
    let library = PersonalLibrary::new();
    let before = library.bytes();
    for kind in [
        EntityKind::Release,
        EntityKind::Artist,
        EntityKind::Recording,
    ] {
        let reference = EntityRef::of(&library.catalog, kind, 0).unwrap();
        for removal in [false, true] {
            let token = reference.to_token();
            let mut args = vec!["played", &token];
            if removal {
                args.push("--remove");
            }
            let result = library.run(&args);
            assert!(
                !result.status.success(),
                "accepted {:?} as a track",
                reference
            );
            assert_eq!(library.bytes(), before);
        }
    }
    let result = library.run(&["played", &library.track.to_token()]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data = library.data();
    assert_eq!(data.play_count(LOCAL_USER, &library.track), 1);
    assert_eq!(data.plays[0].track.kind, EntityKind::Track);
}

#[test]
fn collection_edits_refuse_listing_options_instead_of_silently_ignoring_them() {
    let library = PersonalLibrary::new();
    let mut data = library.data();
    data.save_collection(LOCAL_USER, "Été — 音楽", "title:Song", 1)
        .unwrap();
    library.save(&data);
    let before = library.bytes();
    let output = library.dir.join("ignored.json");
    let output_option = format!("--output={}", output.display());
    for option in [
        "--json",
        "--csv",
        "--m3u",
        "--limit=1",
        "--offset=1",
        "--all",
        "--sort=title",
        "--separator=;",
        output_option.as_str(),
    ] {
        for operation in ["--query=title:Other", "--remove"] {
            let result = library.run(&["collection", "Été — 音楽", operation, option]);
            assert!(
                !result.status.success(),
                "ignored {option} during {operation}"
            );
            assert_eq!(library.bytes(), before);
            assert!(!output.exists());
        }
    }
    let result = library.run(&["collection", "été 音楽", "--json"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        matches!(json::parse(&String::from_utf8(result.stdout).unwrap()), Ok(Json::Arr(rows)) if rows.len() == 1)
    );
}

#[test]
fn favourites_show_a_filled_heart_and_removed_favourites_show_no_heart() {
    let library = PersonalLibrary::new();
    for (kind, command, name) in [
        (EntityKind::Artist, "artist", "Artist"),
        (EntityKind::Release, "album", "Album"),
        (EntityKind::Track, "track", "Song"),
    ] {
        let reference = EntityRef::of(&library.catalog, kind, 0).unwrap().to_token();
        let added = library.run(&["love", &reference]);
        assert!(added.status.success());
        assert!(String::from_utf8_lossy(&added.stdout).contains('❤'));
        let shown = library.run(&[command, name]);
        assert!(shown.status.success());
        assert!(String::from_utf8_lossy(&shown.stdout).contains('❤'));
        let listed = library.run(&["favourites"]);
        assert!(listed.status.success());
        assert!(String::from_utf8_lossy(&listed.stdout).contains('❤'));
        assert!(
            library
                .run(&["love", &reference, "--remove"])
                .status
                .success()
        );
        let removed = library.run(&[command, name]);
        assert!(removed.status.success());
        assert!(!String::from_utf8_lossy(&removed.stdout).contains('❤'));
        assert_eq!(
            library
                .data()
                .find(LOCAL_USER, &library.track)
                .unwrap()
                .note
                .as_deref(),
            Some("preserve this note")
        );
    }
}

#[test]
fn query_and_collection_exports_honour_and_validate_the_requested_window() {
    let library = PersonalLibrary::new();
    let mut data = library.data();
    data.save_collection(LOCAL_USER, "All songs", "title:Song", 1)
        .unwrap();
    library.save(&data);
    let before = library.bytes();

    for command in [["query", "title:Song"], ["collection", "All songs"]] {
        let result = library.run(&[command[0], command[1], "--json", "--offset=1", "--limit=1"]);
        assert!(result.status.success());
        assert!(
            matches!(json::parse(&String::from_utf8(result.stdout).unwrap()), Ok(Json::Arr(rows)) if rows.is_empty()),
            "{} ignored the export offset",
            command[0]
        );
        let result = library.run(&[command[0], command[1], "--csv", "--offset=1", "--limit=1"]);
        assert!(result.status.success());
        assert_eq!(String::from_utf8(result.stdout).unwrap().lines().count(), 1);
        let result = library.run(&[command[0], command[1], "--m3u", "--offset=1"]);
        assert!(!result.status.success(), "an empty M3U must be refused");
        assert!(String::from_utf8_lossy(&result.stderr).contains("nothing to put"));

        for option in ["--limit=0", "--offset=invalid"] {
            for format in ["--json", "--csv", "--m3u"] {
                let result = library.run(&[command[0], command[1], format, option]);
                assert!(
                    !result.status.success(),
                    "{} {format} accepted {option}",
                    command[0]
                );
                assert!(result.stdout.is_empty());
            }
        }
    }
    assert_eq!(library.bytes(), before);
}

#[test]
fn note_copy_resolves_source_names_and_preserves_exact_waiting_references() {
    let library = PersonalLibrary::new();
    let album = EntityRef::of(&library.catalog, EntityKind::Release, 0).unwrap();
    let artist = EntityRef::of(&library.catalog, EntityKind::Artist, 0).unwrap();
    let waiting = EntityRef::new(EntityKind::Track, "/missing/source.flac");
    for (source, token) in [
        (library.track.clone(), "track:Song".into()),
        (artist.clone(), "artist:Artist".into()),
        (album.clone(), "album:Album".into()),
        (album.clone(), "release:Album".into()),
        (library.track.clone(), library.track.to_token()),
        (artist.clone(), artist.to_token()),
        (album.clone(), album.to_token()),
        (waiting.clone(), waiting.to_token()),
    ] {
        let mut data = aede_core::user::UserData::default();
        data.entry(LOCAL_USER, &album, 1).note = Some("replace this destination".into());
        let annotation = data.entry(LOCAL_USER, &source, 1);
        annotation.note = Some("source wording\r\n\r\n".into());
        annotation.rating = Some(4);
        annotation.loved = true;
        annotation.tags.insert("source tag".into());
        data.entry("other", &source, 1).note = Some("other owner's source".into());
        data.entry("other", &album, 1).note = Some("other owner's destination".into());
        let other = data
            .annotations
            .iter()
            .filter(|annotation| annotation.owner == "other")
            .cloned()
            .collect::<Vec<_>>();
        library.save(&data);

        let result = library.run(&["note", "album", "Album", &format!("--from={token}")]);
        assert!(
            result.status.success(),
            "source {token}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let copied = library.data();
        let destination = copied.find(LOCAL_USER, &album).unwrap();
        assert_eq!(destination.note.as_deref(), Some("source wording\r\n\r\n"));
        assert_eq!(destination.rating, Some(4));
        assert!(destination.loved);
        assert!(destination.tags.contains("source tag"));
        assert_eq!(
            copied
                .annotations
                .into_iter()
                .filter(|annotation| annotation.owner == "other")
                .collect::<Vec<_>>(),
            other
        );
    }
}

#[test]
fn note_copy_refuses_ambiguous_or_unknown_source_names_without_mutation() {
    let library = PersonalLibrary::new();
    let mut files = Vec::new();
    for path in ["/music/Album/01.flac", "/music/Other/01.flac"] {
        let mut tags = aede_core::tags::RawTags::default();
        tags.insert("title", "Song");
        tags.insert("artist", "Artist");
        tags.insert("album", "Album");
        files.push(aede_core::model::ScannedFile {
            path: path.into(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    let catalog = aede_core::model::build(files, vec!["/music".into()], 1, &[]);
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&library.dir)).unwrap();
    let before = library.bytes();
    for (source, diagnostic) in [
        ("track:Song", "matches 2 tracks"),
        ("artist:Missing", "no artist matches"),
        ("unknown:Song", "not a kind of thing"),
        ("Song", "--from takes"),
    ] {
        let result = library.run(&[
            "note",
            &library.track.to_token(),
            &format!("--from={source}"),
        ]);
        assert!(!result.status.success(), "accepted source {source}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(diagnostic),
            "source {source}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(library.bytes(), before);
    }
}

#[test]
fn note_append_requires_supplied_text_and_refuses_removal_without_mutation() {
    let library = PersonalLibrary::new();
    let before = library.bytes();
    for options in [vec!["--append", "--remove"], vec!["--append"]] {
        let token = library.track.to_token();
        let mut args = vec!["note", &token];
        args.extend(options);
        let result = library.run(&args);
        assert!(
            !result.status.success(),
            "accepted {args:?}: {}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("--append"));
        assert_eq!(library.bytes(), before);
    }
}

#[test]
fn catalog_selections_refuse_an_output_file_without_a_machine_format() {
    let library = PersonalLibrary::new();
    let mut data = library.data();
    data.save_collection(LOCAL_USER, "All songs", "title:Song", 1)
        .unwrap();
    library.save(&data);
    let before = library.bytes();
    let path = library.dir.join("should-not-be-ignored.json");
    for command in [
        vec!["albums"],
        vec!["artists"],
        vec!["genres"],
        vec!["labels"],
        vec!["years"],
        vec!["favourites"],
        vec!["query", "title:Song"],
        vec!["collection", "All songs"],
        vec!["album", "Album"],
        vec!["artist", "Artist"],
        vec!["track", "Song"],
        vec!["search", "Song"],
    ] {
        let mut args = command.clone();
        args.extend(["--output", path.to_str().unwrap()]);
        let result = library.run(&args);
        assert!(
            !result.status.success(),
            "{command:?} silently ignored --output"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("--output"));
        assert!(!path.exists());
        assert_eq!(library.bytes(), before);
    }
}
