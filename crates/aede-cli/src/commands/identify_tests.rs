//! Tests for [`super`], split out of `identify.rs`.
//!
//! Proved without a network, like every other pass: the service is
//! unreachable from where this was written, so what is pinned is which files
//! are asked about, what is stored, and the two failures that would otherwise
//! be silent.

use super::*;
use crate::commands::fetch::Refusal;
use aede_core::fingerprint::Fingerprint;
use aede_core::json::Json;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::RawTags;

/// A transport answering from canned text, remembering what was asked.
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
        self.asked.push(url.to_string());
        Err(Refusal::Failed("this pass downloads nothing".to_string()))
    }
}

const HEARD: &str = r#"{"status":"ok","results":[{"id":"5c1b","score":0.97,
    "recordings":[{"id":"a3e4f5c6","title":"So What",
      "artists":[{"id":"561d","name":"Miles Davis"}],
      "releasegroups":[{"id":"c9fd","title":"Kind of Blue"}]}]}]}"#;

fn library(fingerprinted: bool) -> Catalog {
    let mut tags = RawTags::default();
    tags.insert("title", "So What");
    tags.insert("artist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    let mut catalog = build(
        vec![ScannedFile {
            path: "/music/Miles/01.flac".to_string(),
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
    if fingerprinted {
        catalog.files[0].fingerprint = Some(Fingerprint {
            data: "AQAA+bc/de".to_string(),
            seconds: 545,
        });
    }
    catalog
}

/// The options a pass reads, with the AcoustID key given rather than exported.
///
/// **The key is a value here because it is a value there.** These tests used to
/// set and unset `AEDE_ACOUSTID_KEY` around each other, which is one variable
/// shared by every thread in a binary that runs its tests on threads: whichever
/// test wrote last won, and the suite passed on a machine with no key and
/// failed on a machine that had one. The environment is read once at the edge
/// now, in `fetch`, and what reaches a pass is a value a test can choose.
fn asked_for(again: bool, dry_run: bool) -> crate::commands::fetch::Asked<'static> {
    with_key(again, dry_run, Some("TESTKEY"))
}

fn with_key(
    again: bool,
    dry_run: bool,
    key: Option<&str>,
) -> crate::commands::fetch::Asked<'static> {
    crate::commands::fetch::Asked {
        scope: &crate::commands::fetch::EVERYTHING,
        names: &[],
        again,
        dry_run,
        size: crate::commands::covers::DEFAULT_SIZE,
        images: false,
        key: key.map(str::to_string),
        langs: vec!["en".to_string()],
    }
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
    let dir = std::env::temp_dir().join(format!("aede_identify_{}_{name}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

#[test]
fn a_file_with_no_fingerprint_is_counted_and_told_where_to_go() {
    // This pass cannot compute one — that is the other command — so the line
    // where it gives up names it.
    let catalog = library(false);
    let found = survey(
        &catalog,
        &sources::Sources::default(),
        &[],
        &crate::commands::fetch::EVERYTHING,
        false,
    );
    assert!(found.targets.is_empty());
    assert_eq!(found.no_fingerprint, 1);
    assert_eq!(waiting(&catalog, &sources::Sources::default()), 0);

    let catalog = library(true);
    assert_eq!(waiting(&catalog, &sources::Sources::default()), 1);
}

#[test]
fn what_it_heard_is_stored_as_a_guess_and_never_as_a_certainty() {
    // The distinction the whole layer is built on: this file was recognised
    // by how it sounds, not named by an identifier its tags carried.
    let dir = sandbox("stored");
    let catalog = library(true);
    let mut layer = sources::Sources::default();
    let mut transport = Canned {
        answers: vec![Ok(HEARD.to_string())],
        asked: Vec::new(),
    };
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked_for(false, false),
    )
    .expect("the pass ran");

    let record = &layer.records[0];
    assert_eq!(record.source, aede_core::acoustid::SOURCE);
    assert!(
        !record.confidence.is_certain(),
        "a fingerprint match is a guess, however good"
    );
    assert_eq!(record.confidence, Confidence::matched(97));
    assert_eq!(record.source_id.as_deref(), Some("a3e4f5c6"));
    match &record.facts {
        Facts::Track(t) => {
            assert_eq!(t.title.as_deref(), Some("So What"));
            assert_eq!(t.artists, vec!["Miles Davis"]);
            assert_eq!(t.album.as_deref(), Some("Kind of Blue"));
            assert_eq!(t.score, Some(97), "a percentage, not a float");
        }
        other => panic!("a track's facts, not {other:?}"),
    }

    // The fingerprint went out escaped, or the service would answer about a
    // file it never saw.
    assert!(
        transport.asked[0].contains("AQAA%2Bbc%2Fde"),
        "{:?}",
        transport.asked
    );

    // Asked once, and not again.
    let found = survey(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        false,
    );
    assert!(found.targets.is_empty());
    assert_eq!(found.asked, 1);
    assert_eq!(
        survey(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            true
        )
        .targets
        .len(),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_the_service_does_not_know_is_recorded_as_asked() {
    // Otherwise every run asks again about the same files for ever — the
    // distinction "asked, and it does not know" from "never asked", which
    // this layer exists to keep.
    let dir = sandbox("unknown");
    let catalog = library(true);
    let mut layer = sources::Sources::default();
    let mut transport = Canned {
        answers: vec![Ok(r#"{"status":"ok","results":[]}"#.to_string())],
        asked: Vec::new(),
    };
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked_for(false, false),
    )
    .expect("the pass ran");

    assert_eq!(layer.records.len(), 1, "the question is recorded as asked");
    assert!(
        survey(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            false
        )
        .targets
        .is_empty()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_key_stops_the_run_instead_of_condemning_the_whole_library() {
    // AcoustID answers `200 OK` with `"status": "error"` for a bad key. Read
    // as an ordinary empty answer, every file would be filed as "asked, and
    // it does not know" — and the reader would conclude their music is
    // unidentifiable. One wrong key is wrong for every file.
    let dir = sandbox("bad_key");
    let catalog = library(true);
    let mut layer = sources::Sources::default();
    let mut transport = Canned {
        answers: vec![Ok(
            r#"{"status":"error","error":{"code":4,"message":"invalid API key"}}"#.to_string(),
        )],
        asked: Vec::new(),
    };
    let error = run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &asked_for(false, false),
    )
    .expect_err("it must stop");
    assert!(error.to_string().contains("invalid API key"), "{error}");
    assert!(
        layer.records.is_empty(),
        "and nothing is filed against the file on the strength of it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_key_is_refused_before_a_single_request() {
    let dir = sandbox("no_key");
    let catalog = library(true);
    let mut layer = sources::Sources::default();
    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    let error = run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        &with_key(false, false, None),
    )
    .expect_err("no key");
    assert!(error.to_string().contains("acoustid.org/new-application"));
    assert!(transport.asked.is_empty(), "nothing was asked");
    let _ = std::fs::remove_dir_all(&dir);
}
