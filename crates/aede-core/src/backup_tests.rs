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
        made_by: "0.1.0".to_string(),
        catalog: Part::Held(catalog()),
        user: Part::Held(user()),
        sources: Part::Held(sources()),
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
    assert_eq!(back.made_by, "0.1.0");
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
        made_by: "0.1.0".to_string(),
        catalog: Part::Empty,
        user: Part::Held(user()),
        sources: Part::Empty,
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
        made_by: "0.1.0".to_string(),
        catalog: Part::Empty,
        user: Part::Empty,
        sources: Part::Empty,
    };
    assert!(nothing.is_empty());
    assert!(!whole().is_empty());
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
