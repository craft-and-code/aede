//! What the discography pass browses, and what the report derives from it.
//!
//! Declared in `discography.rs` with `#[path]`, so this is still that module's
//! own child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::Json;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, Sources};
use aede_core::tags::RawTags;

/// The options a pass reads, as `fetch` gathers them.
fn asked(again: bool) -> crate::commands::fetch::Asked<'static> {
    crate::commands::fetch::Asked {
        names: &[],
        again,
        dry_run: false,
        size: crate::commands::covers::DEFAULT_SIZE,
        images: false,
    }
}
/// A transport that answers from canned text, and remembers what was asked.
struct Canned {
    answers: Vec<Result<String, Refusal>>,
    asked: Vec<String>,
}

impl Ask for Canned {
    fn get_json(&mut self, url: &str) -> Result<Json, Refusal> {
        self.asked.push(url.to_string());
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

/// The folder is named after the **test that owns it**, not after the argument.
/// Three tests once shared one because they shared a helper that named it, and
/// each call begins by deleting it: they raced, passing on Linux and failing on
/// macOS with `Invalid argument`. A name a caller passes is a promise the
/// caller has to keep, and no grep can check it — a helper called from three
/// tests spells the name once. The thread's name is the test's own, so two
/// tests cannot collide however they arrive here, and it is the same on the
/// next run, so a re-run still clears what the last one left.
fn owner(fallback: &str) -> String {
    std::thread::current()
        .name()
        .map(|name| name.replace("::", "_"))
        .unwrap_or_else(|| fallback.to_string())
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_disco_{}", owner(name)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a data folder");
    dir
}

#[test]
fn each_test_gets_a_folder_of_its_own_whatever_it_asks_for() {
    // The mechanism, pinned rather than trusted, because what it prevents is a
    // race that passes here and fails on somebody else's machine: three tests
    // sharing one folder, each deleting it as it starts. The argument is the
    // same word on purpose — the folder is not named by it.
    let mine = sandbox("a name several tests could pass");
    assert!(
        mine.to_string_lossy()
            .contains("each_test_gets_a_folder_of_its_own_whatever_it_asks_for"),
        "named after the test that owns it: {mine:?}"
    );
}

/// A layer holding one identified MusicBrainz artist.
fn held() -> Sources {
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "miles davis".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("561d854a".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            area: Some("United States".to_string()),
            ..Default::default()
        }),
    });
    sources
}

fn discography_of(sources: &Sources) -> Vec<KnownRelease> {
    match &sources.records[0].facts {
        Facts::Artist(a) => a.discography.clone(),
        other => panic!("expected artist facts, got {other:?}"),
    }
}

/// A library holding the albums named, all credited to Miles Davis.
fn library(albums: &[(&str, Option<&str>)]) -> Catalog {
    let files: Vec<ScannedFile> = albums
        .iter()
        .map(|(album, group)| {
            let mut tags = RawTags::default();
            tags.insert("artist", "Miles Davis");
            tags.insert("albumartist", "Miles Davis");
            tags.insert("album", *album);
            tags.insert("title", "A track");
            if let Some(group) = group {
                tags.insert("musicbrainz_releasegroupid", *group);
            }
            ScannedFile {
                path: format!("/music/Miles Davis/{album}/01.flac"),
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
    build(files, vec!["/music".to_string()], 1)
}

/// A library where Miles Davis has an album of his own — the condition both
/// halves of this module require before an artist has anything to be missing.
fn shelf() -> Catalog {
    library(&[("Kind of Blue", Some("g1"))])
}

const PAGE_ONE: &str = r#"{"release-group-count":3,"release-groups":[
    {"id":"g1","title":"Kind of Blue","first-release-date":"1959-08-17",
     "primary-type":"Album","secondary-types":[]},
    {"id":"g2","title":"Bitches Brew","first-release-date":"1970-03-30",
     "primary-type":"Album","secondary-types":[]}]}"#;

const PAGE_TWO: &str = r#"{"release-group-count":3,"release-groups":[
    {"id":"g3","title":"Live-Evil","first-release-date":"1971",
     "primary-type":"Album","secondary-types":["Live"]}]}"#;

#[test]
fn a_discography_longer_than_a_page_is_asked_for_in_pages() {
    let dir = sandbox("paged");
    let path = sources::sources_path(&dir);
    let mut layer = held();
    let mut transport = Canned {
        answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
        asked: Vec::new(),
    };
    run(
        &shelf(),
        &mut transport,
        &[],
        &mut layer,
        &path,
        &asked(false),
    )
    .expect("the pass ran");

    assert_eq!(transport.asked.len(), 2, "asked: {:?}", transport.asked);
    assert!(
        transport.asked[0].contains("offset=0"),
        "{:?}",
        transport.asked
    );
    assert!(
        transport.asked[1].contains("offset=100"),
        "the second page asks for the second page: {:?}",
        transport.asked
    );
    assert_eq!(discography_of(&layer).len(), 3, "both pages were kept");
}

#[test]
fn a_page_that_comes_back_empty_ends_the_walk() {
    // A miscounted total from the service must not turn into a request a
    // second for ever. An empty page ends it whatever the count claims.
    let dir = sandbox("empty_page");
    let path = sources::sources_path(&dir);
    let mut layer = held();
    let mut transport = Canned {
        answers: vec![
            Ok(r#"{"release-group-count":9999,"release-groups":[
                {"id":"g1","title":"One","primary-type":"Album"}]}"#
                .to_string()),
            Ok(r#"{"release-group-count":9999,"release-groups":[]}"#.to_string()),
        ],
        asked: Vec::new(),
    };
    run(
        &shelf(),
        &mut transport,
        &[],
        &mut layer,
        &path,
        &asked(false),
    )
    .expect("the pass ran");
    assert_eq!(transport.asked.len(), 2, "it stopped rather than looping");
}

#[test]
fn the_discography_joins_the_artists_record_and_does_not_replace_it() {
    // The layer is keyed on (entity, source): a second MusicBrainz record
    // about the same artist is not a second opinion but a lost one. The area
    // stored by the first pass has to survive this one.
    let dir = sandbox("joins");
    let path = sources::sources_path(&dir);
    let mut layer = held();
    let mut transport = Canned {
        answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
        asked: Vec::new(),
    };
    run(
        &shelf(),
        &mut transport,
        &[],
        &mut layer,
        &path,
        &asked(false),
    )
    .expect("the pass ran");

    assert_eq!(layer.records.len(), 1, "one source, one row");
    match &layer.records[0].facts {
        Facts::Artist(a) => {
            assert_eq!(
                a.area.as_deref(),
                Some("United States"),
                "what the first pass stored is still there"
            );
            assert_eq!(a.discography.len(), 3);
        }
        other => panic!("expected artist facts, got {other:?}"),
    }

    // And a second run costs nothing.
    let mut second = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&shelf(), &mut second, &[], &mut layer, &path, &asked(false)).expect("the second pass");
    assert!(
        second.asked.is_empty(),
        "already browsed, so not browsed again"
    );
}

#[test]
fn only_what_is_absent_and_a_studio_album_is_reported() {
    let mut layer = held();
    let mut transport = Canned {
        answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
        asked: Vec::new(),
    };
    let dir = sandbox("absent");
    run(
        &shelf(),
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
    )
    .expect("the pass ran");

    // One album on the shelf, matched by its release-group identifier.
    let catalog = library(&[("Kind of Blue", Some("g1"))]);
    let missing = absent(&catalog, &layer, &[], false);
    let titles: Vec<&str> = missing.iter().map(|a| a.known.title.as_str()).collect();
    assert_eq!(
        titles,
        vec!["Bitches Brew"],
        "the one owned is not listed, and the live record is not a gap"
    );
    assert_eq!(missing[0].artist, "Miles Davis");
    assert_eq!(missing[0].known.year(), Some("1970"));
}

#[test]
fn a_shelf_is_recognised_by_title_when_the_tags_carry_no_identifier() {
    // Most libraries are not tagged by Picard. Falling back to the normalised
    // title is what stops the report telling someone they are missing a record
    // that is sitting in front of them — and `normalize` is the same function
    // that decides two spellings are one name everywhere else.
    let mut layer = held();
    let dir = sandbox("bytitle");
    run(
        &shelf(),
        &mut Canned {
            answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
            asked: Vec::new(),
        },
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
    )
    .expect("the pass ran");

    let catalog = library(&[("Kind Of  Blue", None), ("Bitches Brew", None)]);
    assert!(
        absent(&catalog, &layer, &[], false).is_empty(),
        "different spacing and case is the same album"
    );
}

#[test]
fn an_artist_with_no_album_of_their_own_has_no_shelf_to_have_gaps_in() {
    // The report's worst failure, kept as a test. The catalog holds an artist
    // for every credit it reads — a guest on one track, a composer, one name
    // on a compilation. Being in the catalog is not having a place in the
    // library, and the first version answered otherwise: one Rolling Stones
    // track on a compilation produced their entire studio discography as
    // "missing", for every passing credit at once.
    let mut layer = held();
    let dir = sandbox("guest");
    run(
        &shelf(),
        &mut Canned {
            answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
            asked: Vec::new(),
        },
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
    )
    .expect("the pass ran");

    // Miles Davis plays on one track of a compilation and has no album here:
    // the album artist is somebody else entirely.
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Various Artists");
    tags.insert("album", "Jazz Classics");
    tags.insert("title", "So What");
    let compilation = build(
        vec![ScannedFile {
            path: "/music/Compilations/Jazz Classics/01.flac".to_string(),
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
    );
    assert!(
        compilation.artists.iter().any(|a| a.name == "Miles Davis"),
        "he is in the catalog — that is the whole point of the test"
    );
    assert!(
        absent(&compilation, &layer, &[], false).is_empty(),
        "and still has no shelf here, so nothing of his is missing from it"
    );
    assert_eq!(
        waiting(&compilation, &held()),
        0,
        "and he is not browsed either: the two halves have to agree on who has \
         a shelf, or the pass spends a request a second on answers the report \
         will never show"
    );

    // One album of his own, and the report has something to say again.
    let shelf = library(&[("Kind of Blue", None)]);
    assert_eq!(
        absent(&shelf, &layer, &[], false).len(),
        1,
        "a discography that was started is one that can be incomplete"
    );
}

#[test]
fn an_artist_this_catalog_cannot_place_is_not_a_shelf_with_gaps() {
    // A record kept for an artist the catalog does not hold — imported, or
    // left over from a library that has moved — would otherwise have its
    // entire discography reported as missing, which is true of a shelf that
    // never claimed to hold them and drowns the real answer.
    let mut layer = held();
    let dir = sandbox("unplaced");
    run(
        &shelf(),
        &mut Canned {
            answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
            asked: Vec::new(),
        },
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
    )
    .expect("the pass ran");

    let elsewhere = library(&[("Time Out", None)]);
    // The catalog above credits its album to Miles Davis, so to make the
    // artist genuinely absent the catalog has to hold somebody else.
    let mut tags = RawTags::default();
    tags.insert("artist", "Dave Brubeck");
    tags.insert("albumartist", "Dave Brubeck");
    tags.insert("album", "Time Out");
    tags.insert("title", "Take Five");
    let other = build(
        vec![ScannedFile {
            path: "/music/Dave Brubeck/Time Out/01.flac".to_string(),
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
    );
    assert!(!elsewhere.artists.is_empty());
    assert!(
        absent(&other, &layer, &[], false).is_empty(),
        "no shelf, no gaps in it"
    );
}

#[test]
fn a_record_set_aside_leaves_the_report_and_the_source_untouched() {
    // MusicBrainz types a demo, a compilation and a single as `Album` until
    // somebody says otherwise there, and this program will not overrule it —
    // the whole layer exists so that what a source said stays what they said.
    // What it will do is record that the *user* disagrees, and stop showing it.
    let mut layer = held();
    let dir = sandbox("aside");
    run(
        &shelf(),
        &mut Canned {
            answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
            asked: Vec::new(),
        },
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
    )
    .expect("the pass ran");

    let catalog = shelf();
    assert_eq!(absent(&catalog, &layer, &[], false).len(), 1);

    let aside = vec![aede_core::user::SetAside {
        owner: aede_core::user::LOCAL_USER.to_string(),
        release_group: "g2".to_string(),
        title: "Bitches Brew".to_string(),
        created_at: 1,
    }];
    assert!(
        absent(&catalog, &layer, &aside, false).is_empty(),
        "set aside, so no longer reported"
    );

    // And the answer itself is untouched: deleting the record would lose what
    // MusicBrainz said in order to record what the user said, when the two are
    // different claims that this program keeps apart everywhere else.
    let still = discography_of(&layer);
    assert!(
        still.iter().any(|k| k.mbid == "g2"),
        "what the source said is still held: {still:?}"
    );

    // A decision about a record nobody set aside changes nothing.
    let elsewhere = vec![aede_core::user::SetAside {
        release_group: "nothing-like-it".to_string(),
        ..aside[0].clone()
    }];
    assert_eq!(absent(&catalog, &layer, &elsewhere, false).len(), 1);
}

/// The layer after one browse of Miles Davis, for the report to read.
fn browsed() -> Sources {
    let mut layer = held();
    let dir = sandbox("browsed");
    run(
        &shelf(),
        &mut Canned {
            answers: vec![Ok(PAGE_ONE.to_string()), Ok(PAGE_TWO.to_string())],
            asked: Vec::new(),
        },
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked(false),
    )
    .expect("the pass ran");
    layer
}

/// One decision on file: Bitches Brew, set aside.
fn put_aside() -> aede_core::user::UserData {
    aede_core::user::UserData {
        set_aside: vec![aede_core::user::SetAside {
            owner: aede_core::user::LOCAL_USER.to_string(),
            release_group: "g2".to_string(),
            title: "Bitches Brew".to_string(),
            created_at: 1,
        }],
        ..Default::default()
    }
}

#[test]
fn what_is_held_back_can_be_asked_for_and_says_why_it_was() {
    // `--all` means what it means on every listing here: hold nothing back. The
    // report holds back two quite different things — records MusicBrainz does
    // not type as studio albums, and records the reader set aside — and one
    // word lifts both, because they answer one question: *what am I not being
    // shown*. Every row that comes back this way carries its reason; a row that
    // reappeared unmarked would read as an ordinary missing album.
    let layer = browsed();
    let catalog = shelf();

    // Two studio albums are credited and one is on the shelf, so the ordinary
    // report has one row and the live record is not in it.
    let ordinary = absent(&catalog, &layer, &[], false);
    assert_eq!(ordinary.len(), 1);
    assert_eq!(ordinary[0].known.title, "Bitches Brew");
    assert!(
        ordinary[0].held_back.is_none(),
        "a row on the ordinary report is not held back by anything"
    );

    let all = absent(&catalog, &layer, &[], true);
    let rows: Vec<(&str, Option<&str>)> = all
        .iter()
        .map(|a| (a.known.title.as_str(), a.held_back.as_deref()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("Bitches Brew", None),
            // MusicBrainz's own words, not a vocabulary of ours: the reader's
            // next move is often to go and correct the type on the page where
            // it is set.
            ("Live-Evil", Some("Album · Live")),
        ]
    );

    // The reader's own decision is the second reason, and reads as itself.
    let user = put_aside();
    assert!(
        !absent(&catalog, &layer, &user.set_aside, false)
            .iter()
            .any(|a| a.known.title == "Bitches Brew"),
        "set aside, so held back from the ordinary report"
    );
    let all = absent(&catalog, &layer, &user.set_aside, true);
    let brew = all
        .iter()
        .find(|a| a.known.title == "Bitches Brew")
        .expect("produced on request");
    assert_eq!(brew.held_back.as_deref(), Some("set aside"));
    assert!(brew.set_aside);

    // And a record can be both, in which case both are said: naming one would
    // answer half the question.
    let both = vec![aede_core::user::SetAside {
        owner: aede_core::user::LOCAL_USER.to_string(),
        release_group: "g3".to_string(),
        title: "Live-Evil".to_string(),
        created_at: 1,
    }];
    let all = absent(&catalog, &layer, &both, true);
    let live = all
        .iter()
        .find(|a| a.known.title == "Live-Evil")
        .expect("still there");
    assert_eq!(live.held_back.as_deref(), Some("Album · Live, set aside"));
}

#[test]
fn a_name_given_to_list_narrows_it_and_a_name_that_reaches_nothing_says_so() {
    // `aede missing "MIKA" --list` used to list every decision on file and say
    // nothing at all about the word — the fourth time a command here swallowed
    // its argument. The matching lives in `fetch::reaches`, which is what makes
    // this the same behaviour as every other listing rather than a fourth
    // private copy of it.
    let layer = browsed();
    let catalog = shelf();
    let user = put_aside();

    let all = aside_rows(&catalog, &layer, &user, &[]);
    assert_eq!(all.len(), 1, "no name given, so nothing is narrowed away");

    // By title, and by the artist the row belongs to — the second only works
    // because the artist is resolved rather than left off the row.
    let by_title = aside_rows(&catalog, &layer, &user, &["brew".to_string()]);
    assert_eq!(by_title.len(), 1);
    let by_artist = aside_rows(&catalog, &layer, &user, &["miles".to_string()]);
    assert_eq!(by_artist.len(), 1);
    assert!(
        aside_rows(&catalog, &layer, &user, &["mika".to_string()]).is_empty(),
        "a name that reaches nothing narrows to nothing, and the command says \
         so rather than listing everything"
    );
}

#[test]
fn a_set_aside_row_names_the_artist_it_belongs_to() {
    // Kept on nothing: a set-aside is keyed on the release group, which is
    // globally unique, so the artist is derived from the discography that named
    // it. "Sweet Dreams" on its own tells a reader nothing about whose decision
    // they are looking at.
    let layer = browsed();
    let catalog = shelf();
    assert_eq!(whose(&catalog, &layer, "g2"), "Miles Davis");

    // And a decision whose discography has since been forgotten still shows:
    // an undone fetch must not erase a choice the reader made.
    assert_eq!(whose(&catalog, &Sources::default(), "g2"), "");
    let user = aede_core::user::UserData {
        set_aside: vec![aede_core::user::SetAside {
            owner: aede_core::user::LOCAL_USER.to_string(),
            release_group: "nobody-knows".to_string(),
            title: "Sweet Dreams".to_string(),
            created_at: 1,
        }],
        ..Default::default()
    };
    let rows = aside_rows(&catalog, &layer, &user, &[]);
    assert_eq!(rows.len(), 1, "the row survives the artist being unknown");
    assert_eq!(rows[0].1, "");
}

#[test]
fn an_artist_with_no_identifier_cannot_be_browsed() {
    let mut layer = Sources::default();
    layer.set(SourceRecord {
        key: "someone".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::matched(80),
        facts: Facts::Artist(ArtistFacts::default()),
    });
    assert_eq!(waiting(&shelf(), &layer), 0, "nothing to browse by");
}
