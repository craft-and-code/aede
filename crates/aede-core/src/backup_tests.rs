//! Tests for [`super`], split out of `backup.rs`.
//!
//! What is worth pinning here is not that JSON round-trips — three modules
//! already prove that about themselves — but the two decisions this envelope
//! makes: that the stores are nested **unchanged**, and that each of them is
//! refused **on its own**.

use super::*;
use crate::model::builder::{ScannedFile, build};
use crate::sources::{ArtistFacts, Confidence, Facts, SourceRecord};
use crate::tags::RawTags;

#[cfg(unix)]
#[test]
fn credential_archives_recheck_the_permissions_of_the_opened_source() {
    use std::os::unix::fs::PermissionsExt;
    let directory = temporary_directory("changed_read_permissions");
    let path = directory.join("backup.json");
    let mut backup = whole();
    backup.accounts = Part::Held(
        crate::accounts::Accounts::bootstrap("operator", "a long test passphrase", 10).unwrap(),
    );
    write(&backup, &path).unwrap();
    let result = read_with_open(&path, |path| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))?;
        let file = std::fs::File::open(path)?;
        // A private pathname after opening must not legitimize the different,
        // public descriptor that actually supplied the credential bytes.
        let replacement = path.with_extension("replacement");
        write(&backup, &replacement).unwrap();
        std::fs::rename(replacement, path)?;
        Ok(file)
    });
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        result.is_err(),
        "credentials must be protected on the descriptor read"
    );
}

#[test]
fn backup_read_refuses_a_source_replaced_before_it_is_opened() {
    let directory = temporary_directory("replaced_read_source");
    let path = directory.join("backup.json");
    write(&whole(), &path).unwrap();
    let result = read_with_open(&path, |path| {
        let replacement = path.with_extension("replacement");
        let mut other = whole();
        other.made_by = "a different source".into();
        write(&other, &replacement).unwrap();
        std::fs::rename(replacement, path)?;
        std::fs::File::open(path)
    });
    std::fs::remove_dir_all(directory).unwrap();
    assert!(result.is_err(), "replaced archives require a fresh read");
}

#[test]
fn a_growing_backup_read_stops_at_the_inspected_length_and_refuses_short_reads() {
    let mut grown = std::io::Cursor::new(vec![b' '; 1024]);
    assert!(read_document(&mut grown, 4).is_err());
    assert_eq!(
        grown.position(),
        5,
        "an extending source cannot grow the allocation without bound"
    );
    let mut truncated = std::io::Cursor::new(vec![b' '; 3]);
    assert!(read_document(&mut truncated, 4).is_err());
    let mut complete = std::io::Cursor::new(vec![b' '; 4]);
    assert_eq!(read_document(&mut complete, 4).unwrap(), "    ");
}

fn temporary_directory(name: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "aede_backup_{name}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    directory
}

#[cfg(unix)]
#[test]
fn a_linked_backup_destination_cannot_overwrite_another_file() {
    let directory = temporary_directory("linked_output");
    let victim = directory.join("original");
    let output = directory.join("backup.json");
    std::fs::write(&victim, b"original bytes").unwrap();
    std::os::unix::fs::symlink(&victim, &output).unwrap();
    assert!(write(&whole(), &output).is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"original bytes");
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn writing_a_backup_separates_a_destination_hard_link_from_its_original() {
    let directory = temporary_directory("hardlinked_output");
    let original = directory.join("original");
    let output = directory.join("backup.json");
    std::fs::write(&original, b"original bytes").unwrap();
    std::fs::hard_link(&original, &output).unwrap();
    write(&whole(), &output).unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), b"original bytes");
    assert!(read(&output).unwrap().user.held().is_some());
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn newly_written_backups_keep_personal_data_private() {
    use std::os::unix::fs::PermissionsExt;
    let directory = temporary_directory("private_output");
    let output = directory.join("backup.json");
    write(&whole(), &output).unwrap();
    let mode = std::fs::metadata(&output).unwrap().permissions().mode();
    assert_eq!(
        mode & 0o077,
        0,
        "other users must not read personal backup data"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_missing_backup_parent_is_refused_without_creating_a_new_folder() {
    let directory = temporary_directory("missing_parent");
    let parent = directory.join("unmounted-player");
    assert!(write(&whole(), &parent.join("backup.json")).is_err());
    assert!(!parent.exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn replacement_keeps_existing_backup_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let directory = temporary_directory("existing_permissions");
    let output = directory.join("backup.json");
    std::fs::write(&output, b"old archive").unwrap();
    std::fs::set_permissions(&output, std::fs::Permissions::from_mode(0o640)).unwrap();
    write(&whole(), &output).unwrap();
    assert_eq!(
        std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(read(&output).unwrap().user.held().is_some());
    std::fs::remove_dir_all(directory).unwrap();
}

/// A catalog of one track.
fn catalog() -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    build(
        vec![ScannedFile {
            path: "/music/Miles Davis/Kind of Blue/01.flac".to_string(),
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

fn user() -> UserData {
    UserData {
        set_aside: vec![crate::user::SetAside {
            owner: crate::user::LOCAL_USER.to_string(),
            release_group: "g2".to_string(),
            title: "Bitches Brew".to_string(),
            created_at: 1,
        }],
        ..Default::default()
    }
}

fn sources() -> Sources {
    let mut held = Sources::default();
    held.set(SourceRecord {
        key: "miles davis".to_string(),
        source: crate::sources::MUSICBRAINZ.to_string(),
        source_id: Some("561d854a".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            area: Some("United States".to_string()),
            country_code: Some("US".to_string()),
            ..Default::default()
        }),
    });
    held.set_review(crate::sources::SourceReview {
        entity: crate::user::EntityRef::new(crate::model::EntityKind::Artist, "miles davis"),
        source: crate::sources::MUSICBRAINZ.to_string(),
        source_id: Some("561d854a".to_string()),
        decision: crate::sources::ReviewDecision::Accepted,
        reviewed_at: 2,
    });
    held
}

fn whole() -> Backup {
    Backup {
        made_at: 1_700_000_000,
        made_by: "0.3.0".to_string(),
        catalog: Part::Held(catalog()),
        conclusions: Part::Held(Conclusions::default()),
        user: Part::Held(user()),
        sources: Part::Held(sources()),
        accounts: Part::Empty,
    }
}

#[test]
fn credentials_are_independently_versioned_and_old_envelopes_hold_none() {
    let accounts =
        crate::accounts::Accounts::bootstrap("operator", "a long test passphrase", 10).unwrap();
    let mut backup = whole();
    backup.accounts = Part::Held(accounts.clone());
    let mut document = to_json(&backup);
    assert_eq!(
        document.get("accounts"),
        Some(&crate::accounts::to_json(&accounts))
    );
    assert_eq!(
        from_json(&document).unwrap().accounts.held(),
        Some(&accounts)
    );
    document.set("accounts", Json::obj());
    let readable = from_json(&document).unwrap();
    assert!(matches!(readable.accounts, Part::Unreadable(_)));
    assert!(readable.user.held().is_some());
    for version in [1_u32, 2] {
        document.set("format_version", version.into());
        assert!(matches!(
            from_json(&document).unwrap().accounts,
            Part::Empty
        ));
    }
}

#[test]
fn the_three_stores_are_nested_exactly_as_they_write_themselves() {
    // The decision this file rests on. Nothing here re-encodes a catalog, so a
    // field added to the catalog tomorrow is in the backup tomorrow with no
    // second writer to forget it — a fault that would only be discovered by
    // somebody restoring one.
    let document = to_json(&whole());
    for (key, theirs) in [
        ("catalog", crate::store::to_json(&catalog())),
        ("user", crate::user::to_json(&user())),
        ("sources", crate::sources::to_json(&sources())),
    ] {
        assert_eq!(
            document.get(key).map(Json::to_string_pretty),
            Some(theirs.to_string_pretty()),
            "{key} is nested byte for byte as its own module writes it"
        );
    }

    // And it comes back the same way.
    let back = from_json(&document).expect("a backup");
    assert_eq!(back.made_at, 1_700_000_000);
    assert_eq!(back.made_by, "0.3.0");
    assert_eq!(back.catalog.held().map(|c| c.tracks.len()), Some(1));
    assert_eq!(back.user.held().map(|u| u.set_aside.len()), Some(1));
    assert_eq!(back.sources.held().map(|s| s.records.len()), Some(1));
    assert_eq!(back.sources.held().map(|s| s.reviews.len()), Some(1));
}

#[test]
fn a_store_this_build_refuses_does_not_take_the_other_two_with_it() {
    // The reason the version check is not at the top. A backup written by an
    // Aède whose *catalog* format has since moved still holds the notes, which
    // nothing on earth can rebuild, and the fetched layer, which costs twenty
    // minutes of polite requests. Refusing the file outright would throw away
    // the irreplaceable third in order to protect the rebuildable one.
    let mut document = to_json(&whole());
    let mut ancient = crate::store::to_json(&catalog());
    ancient.set("format_version", (crate::store::FORMAT_VERSION + 1).into());
    document.set("catalog", ancient);

    let back = from_json(&document).expect("the envelope is still readable");
    match &back.catalog {
        Part::Unreadable(why) => assert!(
            why.contains("catalog"),
            "and it says so in the catalog's own words: {why}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(back.user.held().is_some(), "the notes are still there");
    assert!(back.sources.held().is_some(), "and so is the layer");
}

#[test]
fn an_envelope_of_another_version_is_refused_outright() {
    // The one check that *is* fatal, and the difference is worth stating: a
    // store this build cannot read sits inside a shape it understands, so the
    // rest can be reached. A shape it does not understand is not a shape to
    // guess at.
    let mut document = to_json(&whole());
    document.set("format_version", (BACKUP_FORMAT_VERSION + 1).into());
    assert!(from_json(&document).is_err());
    assert!(
        from_json(&Json::obj()).is_err(),
        "and a document that is not a backup at all is refused too"
    );
}

#[test]
fn a_store_that_did_not_exist_is_written_as_null_and_read_as_nothing() {
    // The distinction the whole `Part` enum exists for. "There was no
    // sources.json" and "there is one this build refuses" call for opposite
    // things to be done, and a restore that treated the second as the first
    // would silently drop a layer nobody agreed to lose.
    let thin = Backup {
        made_at: 1,
        made_by: "0.3.0".to_string(),
        catalog: Part::Empty,
        conclusions: Part::Empty,
        user: Part::Held(user()),
        sources: Part::Empty,
        accounts: Part::Empty,
    };
    let document = to_json(&thin);
    // Written rather than left out: a key that is simply absent cannot be told
    // from a file that was cut short.
    assert_eq!(document.get("catalog"), Some(&Json::Null));
    assert_eq!(document.get("sources"), Some(&Json::Null));

    let back = from_json(&document).expect("a backup");
    assert!(matches!(back.catalog, Part::Empty));
    assert!(matches!(back.sources, Part::Empty));
    assert!(back.user.held().is_some());
    assert!(!back.is_empty(), "one store is worth backing up");
}

#[test]
fn a_backup_of_nothing_at_all_says_so() {
    // Writing an empty document and reporting success would teach a reader that
    // the command works — which is exactly the belief that costs them the
    // library later.
    let nothing = Backup {
        made_at: 1,
        made_by: "0.3.0".to_string(),
        catalog: Part::Empty,
        conclusions: Part::Empty,
        user: Part::Empty,
        sources: Part::Empty,
        accounts: Part::Empty,
    };
    assert!(nothing.is_empty());
    assert!(!whole().is_empty());
}

#[test]
fn version_one_backup_recovers_embedded_conclusions() {
    let mut document = to_json(&whole());
    document.set("format_version", 1u32.into());
    let mut catalog = crate::store::to_json(&catalog());
    let mut file = catalog.get("file").unwrap().as_arr().unwrap()[0].clone();
    let mut verdict = Json::obj();
    verdict.set("state", "intact".into());
    verdict.set("method", "flac-frame-crc".into());
    file.set("integrity", verdict);
    catalog.set("file", Json::Arr(vec![file]));
    document.set("catalog", catalog);
    let restored = from_json(&document).unwrap();
    assert_eq!(restored.conclusions.held().unwrap().files.len(), 1);
    assert!(restored.user.held().is_some());
}

#[test]
fn it_goes_to_disk_and_comes_back() {
    let dir = std::env::temp_dir().join(format!(
        "aede_backup_{}",
        std::thread::current()
            .name()
            .unwrap_or("main")
            .replace("::", "_")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let path = dir.join("backup.json");

    write(&whole(), &path).expect("written");
    let text = std::fs::read_to_string(&path).expect("readable");
    assert!(
        text.contains('\n'),
        "pretty rather than compact: this file exists to be opened by a worried \
         person, and half of what it is for is that they can see their notes in it"
    );

    let back = read(&path).expect("read back");
    assert_eq!(back.user.held().map(|u| u.set_aside.len()), Some(1));

    // A file that is not JSON at all is an error, not an empty backup.
    std::fs::write(&path, "not a backup").expect("written");
    assert!(read(&path).is_err());
    assert!(read(&dir.join("nothing here.json")).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}
