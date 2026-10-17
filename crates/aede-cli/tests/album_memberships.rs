//! Album line-ups retain uncertainty when no member can be dated.

#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[path = "fetch_output_support/mod.rs"]
mod support;

use aede_core::{conclusions, model, sources, store, tags::RawTags, user};
use sources::{ArtistFacts, Confidence, Facts, Membership, Side, SourceRecord, Sources};
use support::Library;

fn catalog(library: &Library, dated: bool, compilation: bool) -> model::Catalog {
    let original = store::load(&store::catalog_path(&library.data))
        .unwrap()
        .unwrap();
    let mut tags = RawTags {
        fields: original.files[0].tags.clone(),
        ..Default::default()
    };
    if dated {
        tags.insert("date", "2000");
    }
    if compilation {
        tags.insert("compilation", "1");
    }
    let catalog = model::build(
        vec![model::ScannedFile {
            path: library.audio.to_string_lossy().into_owned(),
            size: 24,
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
    user::save(&user::UserData::default(), &user::user_path(&library.data)).unwrap();
    conclusions::save(
        &conclusions::Conclusions::default(),
        &conclusions::conclusions_path(&library.data),
    )
    .unwrap();
    catalog
}

fn member(name: &str, began: Option<&str>, ended: Option<&str>, over: Option<bool>) -> Membership {
    Membership {
        relation_id: Some(format!("membership-{name}")),
        role_id: None,
        direction: Some("backward".into()),
        mbid: format!("artist-{name}"),
        name: name.into(),
        credited_as: None,
        side: Side::Player,
        kind: "member of band".into(),
        attributes: vec!["guitar".into()],
        began: began.map(str::to_string),
        ended: ended.map(str::to_string),
        over,
    }
}

fn record(catalog: &model::Catalog, members: Vec<Membership>) -> SourceRecord {
    let artist = catalog.find_artist("Miles Davis").unwrap();
    SourceRecord {
        key: artist.key.clone(),
        source: sources::MUSICBRAINZ.into(),
        source_id: artist.mbid.clone(),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            members,
            ..Default::default()
        }),
    }
}

fn save_record(library: &Library, record: SourceRecord) {
    let held = Sources {
        records: vec![record],
        ..Default::default()
    };
    sources::save(&held, &sources::sources_path(&library.data)).unwrap();
}

fn read(library: &Library, arguments: &[&str], success: bool) -> String {
    let before = filesystem::snapshot(&library.data);
    let music_before = filesystem::snapshot(&library.music);
    let output = library.run(arguments);
    assert_eq!(
        output.status.success(),
        success,
        "{arguments:?}: {output:?}"
    );
    assert_eq!(filesystem::snapshot(&library.data), before, "{arguments:?}");
    assert_eq!(filesystem::snapshot(&library.music), music_before);
    format!(
        "{}{}",
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap()
    )
}

#[test]
fn album_lineups_report_unplaceable_members_even_without_a_dated_player() {
    const UNKNOWN: &str = "1 musician MusicBrainz does not date closely enough to place";
    let unplaced = || member("Unplaced Player", None, None, None);
    let current = || member("Current Player", Some("1990"), None, Some(false));
    let cases = [
        ("no-start", vec![unplaced()], false, true),
        (
            "ended-without-date",
            vec![member("Unplaced Player", Some("1990"), None, Some(true))],
            false,
            true,
        ),
        (
            "start-only",
            vec![member("Unplaced Player", Some("1990"), None, None)],
            false,
            true,
        ),
        ("mixed", vec![unplaced(), current()], true, true),
        ("current", vec![current()], true, false),
        (
            "past",
            vec![member(
                "Past Player",
                Some("1980"),
                Some("1990"),
                Some(true),
            )],
            false,
            false,
        ),
        ("empty", vec![], false, false),
    ];
    for (case, members, placed, unknown) in cases {
        let library = Library::new(case, false);
        let catalog = catalog(&library, true, false);
        save_record(&library, record(&catalog, members));
        let page = read(&library, &["album", case], true);
        assert_eq!(page.contains(UNKNOWN), unknown, "{case}: {page}");
        assert_eq!(
            page.contains("line-up in 2000: Current Player"),
            placed,
            "{case}: {page}"
        );
        assert!(!page.contains("Unplaced Player"), "{case}: {page}");
        if !placed {
            assert!(!page.contains("line-up in 2000:"), "{case}: {page}");
        }
        let membership = read(&library, &["artist", "Miles Davis", "--members"], true);
        assert_eq!(membership.contains("Unplaced Player"), unknown);
    }

    for (case, dated, compilation) in [("no-year", false, false), ("compilation", true, true)] {
        let library = Library::new(case, false);
        let catalog = catalog(&library, dated, compilation);
        save_record(&library, record(&catalog, vec![unplaced(), current()]));
        let page = read(&library, &["album", case], true);
        assert!(
            !page.contains(UNKNOWN) && !page.contains("line-up in"),
            "{page}"
        );
    }

    let library = Library::new("reviewed", false);
    let catalog = catalog(&library, true, false);
    let record = SourceRecord {
        confidence: Confidence::matched(93),
        ..record(&catalog, vec![unplaced()])
    };
    save_record(&library, record.clone());
    let held = sources::load_all(&sources::sources_path(&library.data))
        .unwrap()
        .unwrap();
    let review = held.review_items(&catalog)[0].id.clone();
    let pending = read(&library, &["album", "reviewed"], true);
    assert!(!pending.contains(UNKNOWN));
    read(&library, &["artist", "Miles Davis", "--members"], false);
    for (action, trusted) in [("accept", true), ("reject", false), ("undo", false)] {
        let music_before = filesystem::snapshot(&library.music);
        let argument = format!("--{action}={review}");
        let output = library.run(&["review", &argument]);
        assert!(output.status.success(), "{action}: {output:?}");
        assert_eq!(filesystem::snapshot(&library.music), music_before);
        let page = read(&library, &["album", "reviewed"], true);
        assert_eq!(page.contains(UNKNOWN), trusted, "{action}: {page}");
        assert!(!page.contains("line-up in 2000:"));
        let held = sources::load_all(&sources::sources_path(&library.data))
            .unwrap()
            .unwrap();
        assert_eq!(
            held.get(&record.entity(), sources::MUSICBRAINZ),
            Some(&record)
        );
    }
}

#[cfg(unix)]
#[test]
fn artist_followup_commands_preserve_literal_names() {
    use std::process::Command;

    let capture = |command: &str, expected: &[&str]| {
        // Only these fixed harmless fixture names enter this shell function.
        // Capture fetch arguments without contacting a provider.
        let output = Command::new("/bin/sh")
            .args([
                "-c",
                &format!("aede() {{ printf '%s\\n' \"$@\"; }}\n{command}"),
            ])
            .env_clear()
            .env("TEST_WORD", "expanded")
            .output()
            .unwrap();
        assert!(output.status.success(), "{command}: {output:?}");
        let arguments = String::from_utf8(output.stdout).unwrap();
        let arguments: Vec<_> = arguments.lines().collect();
        assert_eq!(arguments, expected, "{command}");
    };
    let command_in = |text: &str, prefix: &str| {
        text.lines()
            .find_map(|line| {
                let start = line.find(prefix)?;
                Some(line[start..].trim().to_string())
            })
            .unwrap_or_else(|| panic!("no {prefix} command: {text}"))
    };

    for name in [
        "Member's $TEST_WORD Band",
        "The \"Quoted\" Band",
        "Ordinary Band",
    ] {
        let library = Library::new("hint-album", false);
        let original = catalog(&library, true, false);
        let mut tags = RawTags {
            fields: original.files[0].tags.clone(),
            ..Default::default()
        };
        for key in ["artist", "albumartist"] {
            tags.fields.remove(key);
            tags.insert(key, name);
        }
        let catalog = model::build(
            vec![model::ScannedFile {
                path: library.audio.to_string_lossy().into_owned(),
                size: 24,
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
        let artist = catalog.find_artist(name).unwrap();
        let record = SourceRecord {
            key: artist.key.clone(),
            source: sources::MUSICBRAINZ.into(),
            source_id: artist.mbid.clone(),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                members: vec![member("Current Player", Some("1990"), None, Some(false))],
                discography: vec![sources::KnownRelease {
                    mbid: "absent-release-group".into(),
                    title: "An Absent Album".into(),
                    first_released: Some("1990".into()),
                    primary_type: Some("Album".into()),
                    secondary_types: vec![],
                }],
                ..Default::default()
            }),
        };
        save_record(&library, record.clone());
        // `missing` admits the shared writer lock even for its read-only list.
        std::fs::write(library.data.join(".aede.lock"), b"").unwrap();
        let page = read(&library, &["artist", name], true);
        let members = command_in(&page, "aede artist ");
        capture(&members, &["artist", name, "--members"]);
        let membership = read(&library, &["artist", name, "--members"], true);
        assert!(membership.contains("Current Player"));
        let missing = command_in(&page, "aede missing ");
        capture(&missing, &["missing", name]);
        let missing = read(&library, &["missing", name], true);
        assert!(missing.contains("An Absent Album"));

        save_record(
            &library,
            SourceRecord {
                facts: Facts::Artist(ArtistFacts::default()),
                ..record
            },
        );
        let empty = read(&library, &["artist", name, "--members"], true);
        let refresh = command_in(&empty, "aede fetch ");
        let refresh = refresh.strip_suffix(" asks again").unwrap();
        capture(refresh, &["fetch", name, "--full"]);
        std::fs::remove_file(sources::sources_path(&library.data)).unwrap();
        let unfetched = read(&library, &["artist", name, "--members"], false);
        let fetch = command_in(&unfetched, "aede fetch ");
        capture(&fetch, &["fetch", name]);
    }
}
