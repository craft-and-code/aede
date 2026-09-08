//! What the lyrics pass asks about, and what it refuses to write.
//!
//! Declared in `lyrics.rs` with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::Json;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::RawTags;

/// A transport answering from canned text, and remembering what was asked.
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

    fn get_bytes(&mut self, _url: &str) -> Result<Vec<u8>, Refusal> {
        // This pass asks questions and downloads nothing; a call here is a
        // mistake worth failing on rather than a case worth answering.
        Err(Refusal::Failed("this pass downloads nothing".to_string()))
    }
}

/// A folder of its own, named after the test **and** the argument.
fn scratch(what: &str) -> std::path::PathBuf {
    let named = std::thread::current()
        .name()
        .map(|n| n.replace("::", "_"))
        .unwrap_or_else(|| "main".to_string());
    let dir = std::env::temp_dir().join(format!("aede_lyrics_{named}_{what}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

/// One track in a folder, with whatever tags a test wants on it.
fn track(dir: &std::path::Path, name: &str, fields: &[(&str, &str)]) -> ScannedFile {
    let path = dir.join(format!("{name}.flac"));
    std::fs::write(&path, b"not really audio").expect("a file");
    let mut tags = RawTags::default();
    tags.insert("artist", "Ozzy Osbourne");
    tags.insert("albumartist", "Ozzy Osbourne");
    tags.insert("album", "Blizzard of Ozz");
    tags.insert("title", name);
    for (key, value) in fields {
        tags.insert(key, *value);
    }
    tags.properties.duration_ms = Some(296_000);
    ScannedFile {
        path: path.to_string_lossy().to_string(),
        size: 1,
        mtime: 1,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

fn shelf(dir: &std::path::Path, files: Vec<ScannedFile>) -> aede_core::model::Catalog {
    build(files, vec![dir.to_string_lossy().to_string()], 1, &[])
}

fn asked_for(names: &[String]) -> crate::commands::fetch::Asked<'_> {
    crate::commands::fetch::Asked {
        scope: &crate::commands::fetch::EVERYTHING,
        names,
        again: false,
        dry_run: false,
        key: None,
        langs: Vec::new(),
        size: crate::commands::covers::DEFAULT_SIZE,
        images: false,
    }
}

fn args() -> Args {
    Args::parse(vec!["fetch".to_string(), "--lyrics".to_string()])
}

const CRAZY: &str = r#"{"plainLyrics":"All aboard","syncedLyrics":"[00:12.00]All aboard"}"#;

#[test]
fn a_track_that_already_has_words_is_never_asked_about() {
    // Neither one carrying them in its tags nor one with a `.lrc` beside it.
    // No request, no file, nothing — the same refusal the cover pass makes,
    // and for the same reason: overwriting what somebody wrote is not
    // something this should be able to do by accident.
    let dir = scratch("has_words");
    let with_tag = track(&dir, "Tagged", &[("lyrics", "All aboard")]);
    let mut with_sidecar = track(&dir, "Sidecarred", &[]);
    with_sidecar.sidecar = Some(dir.join("Sidecarred.lrc").to_string_lossy().to_string());
    let catalog = shelf(&dir, vec![with_tag, with_sidecar]);

    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");
    assert!(transport.asked.is_empty(), "asked: {:?}", transport.asked);
}

#[test]
fn a_track_with_nothing_to_match_on_is_not_asked_about() {
    // The service matches on artist, title, album **and** length. A track
    // missing one of them cannot be looked up, and sending the request anyway
    // would spend a free service's time on a question with no answer.
    let dir = scratch("unaskable");
    let mut no_length = track(&dir, "No length", &[]);
    no_length.tags.properties.duration_ms = None;
    let catalog = shelf(&dir, vec![no_length]);

    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");
    assert!(transport.asked.is_empty(), "asked: {:?}", transport.asked);
}

#[test]
fn an_answer_becomes_a_lrc_beside_its_track_and_nothing_else_is_touched() {
    let dir = scratch("written");
    let one = track(&dir, "Crazy Train", &[]);
    let audio = std::path::PathBuf::from(&one.path);
    let before = std::fs::read(&audio).expect("the audio");
    let catalog = shelf(&dir, vec![one]);

    let mut transport = Canned {
        answers: vec![Ok(CRAZY.to_string())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");

    assert_eq!(transport.asked.len(), 1, "one request per track");
    assert!(
        transport.asked[0].contains("duration=296"),
        "the length is part of the question: {}",
        transport.asked[0]
    );

    let sidecar = dir.join("Crazy Train.lrc");
    let written = std::fs::read_to_string(&sidecar).expect("a lyrics file");
    assert!(written.starts_with("[00:12.00]All aboard"), "{written}");
    assert!(written.ends_with('\n'), "a text file ends with a newline");
    // **The audio is untouched.** The one thing this program never does.
    assert_eq!(std::fs::read(&audio).expect("the audio"), before);
}

#[test]
fn a_file_that_appeared_while_the_run_was_working_is_left_alone() {
    // The survey and the writing are separated by a network. A `.lrc` dropped
    // in by hand, or by a second copy of this program, must not be overwritten
    // by an answer that was asked for before it existed.
    let dir = scratch("raced");
    let one = track(&dir, "Crazy Train", &[]);
    let catalog = shelf(&dir, vec![one]);
    let sidecar = dir.join("Crazy Train.lrc");
    std::fs::write(&sidecar, "mine, thanks\n").expect("a file");

    let mut transport = Canned {
        answers: vec![Ok(CRAZY.to_string())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");
    assert_eq!(
        std::fs::read_to_string(&sidecar).expect("still there"),
        "mine, thanks\n"
    );
}

#[test]
fn a_service_that_has_nothing_is_not_a_failure() {
    // A library of any size holds plenty LRCLIB has never seen: it answers
    // 404, and a run that called that a failure would describe a working
    // service as broken.
    let dir = scratch("missing");
    let one = track(&dir, "Crazy Train", &[]);
    let catalog = shelf(&dir, vec![one]);

    let mut transport = Canned {
        answers: vec![Err(Refusal::Missing)],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");
    assert!(!dir.join("Crazy Train.lrc").exists(), "nothing was written");
}

#[test]
fn an_instrumental_writes_no_file() {
    // The service knows the recording and says it has no words. Writing an
    // empty `.lrc` for it would put a claim on somebody's disk that nobody
    // made, and would then hide the track from every later run.
    let dir = scratch("instrumental");
    let one = track(&dir, "Crazy Train", &[]);
    let catalog = shelf(&dir, vec![one]);

    let mut transport = Canned {
        answers: vec![Ok(r#"{"instrumental":true,"plainLyrics":""}"#.to_string())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");
    assert!(!dir.join("Crazy Train.lrc").exists());
}

#[test]
fn a_dry_run_asks_nothing_and_writes_nothing() {
    let dir = scratch("dry");
    let one = track(&dir, "Crazy Train", &[]);
    let catalog = shelf(&dir, vec![one]);

    let mut asked = asked_for(&[]);
    asked.dry_run = true;
    let mut transport = Canned {
        answers: vec![Ok(CRAZY.to_string())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked).expect("a run");
    assert!(transport.asked.is_empty(), "asked: {:?}", transport.asked);
    assert!(!dir.join("Crazy Train.lrc").exists());
}

#[test]
fn a_name_given_to_the_pass_narrows_it_instead_of_being_swallowed() {
    // The fault this program has made four times: a command taking a word and
    // answering as though nothing had been typed.
    let dir = scratch("named");
    let catalog = shelf(
        &dir,
        vec![
            track(&dir, "Crazy Train", &[]),
            track(&dir, "Mr Crowley", &[]),
        ],
    );

    let names = vec!["crowley".to_string()];
    let mut transport = Canned {
        answers: vec![Ok(CRAZY.to_string())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&names)).expect("a run");
    assert_eq!(transport.asked.len(), 1, "asked: {:?}", transport.asked);
    assert!(
        transport.asked[0].contains("Mr%20Crowley"),
        "the word chose which: {}",
        transport.asked[0]
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "catalog paths are `/`-separated; see docs/design/paths.md"
)]
fn a_folder_narrows_the_pass_to_the_tracks_under_it() {
    // The pass a folder was asked for first, and the case a name cannot
    // answer: both tracks are credited to the same person, so nothing but
    // *where they are* tells them apart.
    // Canonical, as `aede scan` files its roots: on macOS the temp folder is
    // reached through a link, and a catalog filed under the unresolved
    // spelling is one the program cannot build. See `docs/design/paths.md`.
    let dir = super::super::canonical(&scratch("folder"));
    let one = dir.join("Revenge");
    let other = dir.join("Blizzard of Ozz");
    std::fs::create_dir_all(&one).expect("a folder");
    std::fs::create_dir_all(&other).expect("a folder");
    let catalog = shelf(
        &dir,
        vec![
            track(&one, "Slaves of Rot", &[]),
            track(&other, "Crazy Train", &[]),
        ],
    );

    let scope = crate::commands::fetch::Scope::of(&catalog, &[one.to_string_lossy().to_string()])
        .expect("a folder the catalog holds");
    let (targets, _) = survey(&catalog, &[], &scope);
    assert_eq!(
        targets.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec!["Slaves of Rot"]
    );

    // And without one, the whole shelf — the folder narrowed it, nothing else.
    let (all, _) = survey(&catalog, &[], &crate::commands::fetch::EVERYTHING);
    assert_eq!(all.len(), 2);
}
