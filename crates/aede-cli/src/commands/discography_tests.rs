//! What the discography pass browses, and what the report derives from it.
//!
//! Declared in `discography.rs` with `#[path]`, so this is still that module's
//! own child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::Json;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, Sources};
use aede_core::tags::RawTags;

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
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_disco_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a data folder");
    dir
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
    run(&shelf(), &mut transport, &[], &mut layer, &path, false).expect("the pass ran");

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
    run(&shelf(), &mut transport, &[], &mut layer, &path, false).expect("the pass ran");
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
    run(&shelf(), &mut transport, &[], &mut layer, &path, false).expect("the pass ran");

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
    run(&shelf(), &mut second, &[], &mut layer, &path, false).expect("the second pass");
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
        false,
    )
    .expect("the pass ran");

    // One album on the shelf, matched by its release-group identifier.
    let catalog = library(&[("Kind of Blue", Some("g1"))]);
    let missing = absent(&catalog, &layer, &[]);
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
        false,
    )
    .expect("the pass ran");

    let catalog = library(&[("Kind Of  Blue", None), ("Bitches Brew", None)]);
    assert!(
        absent(&catalog, &layer, &[]).is_empty(),
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
        false,
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
        }],
        vec!["/music".to_string()],
        1,
    );
    assert!(
        compilation.artists.iter().any(|a| a.name == "Miles Davis"),
        "he is in the catalog — that is the whole point of the test"
    );
    assert!(
        absent(&compilation, &layer, &[]).is_empty(),
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
        absent(&shelf, &layer, &[]).len(),
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
        false,
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
        }],
        vec!["/music".to_string()],
        1,
    );
    assert!(!elsewhere.artists.is_empty());
    assert!(
        absent(&other, &layer, &[]).is_empty(),
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
        false,
    )
    .expect("the pass ran");

    let catalog = shelf();
    assert_eq!(absent(&catalog, &layer, &[]).len(), 1);

    let aside = vec![aede_core::user::SetAside {
        owner: aede_core::user::LOCAL_USER.to_string(),
        release_group: "g2".to_string(),
        title: "Bitches Brew".to_string(),
        created_at: 1,
    }];
    assert!(
        absent(&catalog, &layer, &aside).is_empty(),
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
    assert_eq!(absent(&catalog, &layer, &elsewhere).len(), 1);
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
