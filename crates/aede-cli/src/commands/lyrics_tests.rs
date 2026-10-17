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
        portrait_key: None,
        langs: Vec::new(),
        size: crate::commands::covers::DEFAULT_SIZE,
        images: false,
        fanart: Default::default(),
        size_requested: false,
    }
}

fn args() -> Args {
    Args::parse(vec!["fetch".to_string(), "--lyrics".to_string()])
}

const CRAZY: &str =
    r#"{"instrumental":false,"plainLyrics":"All aboard","syncedLyrics":"[00:12.00]All aboard"}"#;

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
    std::fs::write(dir.join("Sidecarred.lrc"), "All aboard\n").unwrap();
    let catalog = shelf(&dir, vec![with_tag, with_sidecar]);

    let mut transport = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).expect("a run");
    assert!(transport.asked.is_empty(), "asked: {:?}", transport.asked);
}

#[test]
fn metadata_only_lyrics_do_not_hide_missing_or_readable_words_during_acquisition() {
    let dir = scratch("metadata_only_tag");
    let headers = "[ar:Fixture]\n[ti:Header only]\n[by:Fixture editor]\n[offset:10]";
    let files = vec![
        track(&dir, "Header only", &[("lyrics", headers)]),
        track(&dir, "Plain words", &[("lyrics", "A retained line")]),
        track(&dir, "Timed clearing", &[("lyrics", "[00:01]")]),
        track(&dir, "Existing sidecar", &[("lyrics", headers)]),
    ];
    let audio_before: Vec<_> = files
        .iter()
        .map(|file| (file.path.clone(), std::fs::read(&file.path).unwrap()))
        .collect();
    let existing_sidecar = dir.join("Existing sidecar.lrc");
    std::fs::write(&existing_sidecar, "Existing words stay mine\n").unwrap();
    let catalog = shelf(&dir, files);
    let header_track = catalog
        .tracks
        .iter()
        .find(|track| track.title == "Header only")
        .unwrap();
    assert!(catalog.lyrics_of_track(header_track.id).is_none());
    let tags_before = catalog
        .files
        .iter()
        .map(|file| file.tags.clone())
        .collect::<Vec<_>>();
    let mut transport = Canned {
        answers: vec![Ok(CRAZY.to_string())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).unwrap();
    assert_eq!(transport.asked.len(), 1);
    assert!(transport.asked[0].contains("Header%20only"));
    let acquired = dir.join("Header only.lrc");
    assert_eq!(
        aede_core::lyrics::read(&acquired).unwrap().text(),
        "All aboard"
    );
    assert!(!dir.join("Plain words.lrc").exists());
    assert!(!dir.join("Timed clearing.lrc").exists());
    assert_eq!(
        std::fs::read_to_string(&existing_sidecar).unwrap(),
        "Existing words stay mine\n"
    );
    let (targets, skipped) = survey(&catalog, &[], &crate::commands::fetch::EVERYTHING);
    assert!(
        targets.is_empty(),
        "the new sidecar suppresses another request"
    );
    assert_eq!(skipped.tagged, 2);
    assert_eq!(skipped.sidecar, 2);
    assert_eq!(
        catalog
            .files
            .iter()
            .map(|file| file.tags.clone())
            .collect::<Vec<_>>(),
        tags_before
    );
    for (path, bytes) in audio_before {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    let _ = std::fs::remove_dir_all(dir);

    for (name, answer, expected) in [
        (
            "plain_fallback",
            r#"{"instrumental":false,"plainLyrics":"Plain words remain intact","syncedLyrics":"[ar:Fixture]\n[ti:Title]"}"#,
            Some("Plain words remain intact\n"),
        ),
        (
            "metadata_response",
            r#"{"instrumental":false,"plainLyrics":null,"syncedLyrics":"[ar:Fixture]\n[ti:Title]"}"#,
            None,
        ),
        (
            "clearing_response",
            r#"{"instrumental":false,"plainLyrics":null,"syncedLyrics":"[00:01]"}"#,
            Some("[00:01]\n"),
        ),
    ] {
        let dir = scratch(name);
        let file = track(&dir, name, &[]);
        let audio_before = std::fs::read(&file.path).unwrap();
        let path = file.path.clone();
        let catalog = shelf(&dir, vec![file]);
        let mut transport = Canned {
            answers: vec![Ok(answer.into())],
            asked: Vec::new(),
        };
        run(&args(), &catalog, &mut transport, &[], &asked_for(&[])).unwrap();
        assert_eq!(transport.asked.len(), 1);
        let sidecar = dir.join(format!("{name}.lrc"));
        if let Some(expected) = expected {
            assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), expected);
            assert!(aede_core::lyrics::read(&sidecar).is_some());
        } else {
            assert!(!sidecar.exists(), "metadata is not a completed lyrics file");
            let mut retry = Canned {
                answers: vec![Ok(CRAZY.into())],
                asked: Vec::new(),
            };
            run(&args(), &catalog, &mut retry, &[], &asked_for(&[])).unwrap();
            assert_eq!(retry.asked.len(), 1);
            assert_eq!(
                aede_core::lyrics::read(&sidecar).unwrap().text(),
                "All aboard"
            );
        }
        assert_eq!(std::fs::read(path).unwrap(), audio_before);
        let _ = std::fs::remove_dir_all(dir);
    }
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
fn freshly_written_lyrics_are_not_requested_again_before_a_rescan() {
    let dir = scratch("resume");
    let catalog = shelf(&dir, vec![track(&dir, "Crazy Train", &[])]);
    let mut first = Canned {
        answers: vec![Ok(CRAZY.into())],
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut first, &[], &asked_for(&[])).unwrap();
    let sidecar = dir.join("Crazy Train.lrc");
    let before = std::fs::read(&sidecar).unwrap();
    let mut retry = Canned {
        answers: Vec::new(),
        asked: Vec::new(),
    };
    run(&args(), &catalog, &mut retry, &[], &asked_for(&[])).unwrap();
    assert!(
        retry.asked.is_empty(),
        "the disk already has the successful answer"
    );
    assert_eq!(std::fs::read(sidecar).unwrap(), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lyrics_need_a_positive_whole_second_duration_for_matching() {
    let dir = scratch("zero_length");
    let files = [0, 999]
        .into_iter()
        .map(|duration| {
            let mut file = track(&dir, &format!("Track {duration}"), &[]);
            file.tags.properties.duration_ms = Some(duration);
            file
        })
        .collect();
    let catalog = shelf(&dir, files);
    let (targets, skipped) = survey(&catalog, &[], &crate::commands::fetch::EVERYTHING);
    assert!(targets.is_empty());
    assert_eq!(skipped.unaskable, 2);
    let _ = std::fs::remove_dir_all(dir);
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
        answers: vec![Ok(
            r#"{"instrumental":true,"plainLyrics":"","syncedLyrics":null}"#.to_string(),
        )],
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

#[test]
fn a_removed_catalogued_sidecar_does_not_hide_missing_lyrics_from_acquisition() {
    let cases = [
        ("removed", true, false, "absent"),
        ("removed_uppercase", true, true, "absent"),
        ("never_present", false, false, "absent"),
        ("present", true, false, "file"),
        ("present_uppercase", true, true, "file"),
        ("present_unremembered", false, false, "file"),
        ("directory", true, false, "directory"),
        ("directory_unremembered", false, false, "directory"),
    ];
    #[cfg(unix)]
    let cases = cases.into_iter().chain([
        ("dangling_link", true, false, "link"),
        ("dangling_link_unremembered", false, false, "link"),
    ]);
    for (name, remembered, uppercase, kind) in cases {
        for again in [false, true] {
            let dir = scratch(&format!("{name}_{again}"));
            let mut file = track(&dir, "Retry Track", &[]);
            let audio = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../aede-core/tests/fixtures/track.flac"
            ));
            std::fs::write(&file.path, audio).unwrap();
            let canonical = aede_core::lyrics::sidecar_of(std::path::Path::new(&file.path));
            let sidecar = canonical.with_extension(if uppercase { "LRC" } else { "lrc" });
            let original = "[00:01]Words supplied by the user\n";
            if remembered {
                std::fs::write(&sidecar, original).unwrap();
                file.sidecar = Some(sidecar.to_string_lossy().into_owned());
                std::fs::remove_file(&sidecar).unwrap();
            }
            match kind {
                "file" => std::fs::write(&sidecar, original).unwrap(),
                "directory" => std::fs::create_dir(&sidecar).unwrap(),
                #[cfg(unix)]
                "link" => std::os::unix::fs::symlink(dir.join("absent_target"), &sidecar).unwrap(),
                _ => {}
            }
            let catalog = shelf(&dir, vec![file]);
            let catalog_path = aede_core::store::catalog_path(&dir.join("data"));
            aede_core::store::save(&catalog, &catalog_path).unwrap();
            let catalog_before = std::fs::read(&catalog_path).unwrap();
            let tags_before = catalog.files[0].tags.clone();
            let mut transport = Canned {
                answers: vec![Ok(CRAZY.into())],
                asked: Vec::new(),
            };
            let mut asked = asked_for(&[]);
            asked.again = again;
            run(&args(), &catalog, &mut transport, &[], &asked).unwrap();
            if kind == "absent" {
                assert_eq!(
                    transport.asked.len(),
                    1,
                    "{name}/{again}: a removed path is not existing lyrics"
                );
                assert_eq!(
                    aede_core::lyrics::read(&canonical).unwrap().text(),
                    "All aboard"
                );
                let words_before = std::fs::read(&canonical).unwrap();
                let mut retry = Canned {
                    answers: Vec::new(),
                    asked: Vec::new(),
                };
                run(&args(), &catalog, &mut retry, &[], &asked).unwrap();
                assert!(retry.asked.is_empty(), "a successful retry is reused");
                assert_eq!(std::fs::read(&canonical).unwrap(), words_before);
            } else {
                assert!(transport.asked.is_empty(), "{name}/{again}");
                match kind {
                    "file" => assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), original),
                    "directory" => assert!(sidecar.is_dir()),
                    #[cfg(unix)]
                    "link" => assert_eq!(
                        std::fs::read_link(&sidecar).unwrap(),
                        dir.join("absent_target")
                    ),
                    _ => unreachable!(),
                }
            }
            assert_eq!(std::fs::read(&catalog.files[0].path).unwrap(), audio);
            assert_eq!(catalog.files[0].tags, tags_before);
            assert_eq!(std::fs::read(&catalog_path).unwrap(), catalog_before);
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[test]
fn lyrics_album_names_select_tracks_without_bypassing_folder_or_existing_words() {
    let dir = crate::commands::canonical(&scratch("album_selection"));
    let first = dir.join("first");
    let second = dir.join("second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let mut other = track(&second, "Another Song", &[]);
    for (field, value) in [
        ("artist", "Other Performer"),
        ("albumartist", "Other Performer"),
        ("album", "Other Album"),
    ] {
        other.tags.fields.insert(field.into(), vec![value.into()]);
    }
    let catalog = shelf(
        &dir,
        vec![
            track(&first, "Crazy Train", &[]),
            track(
                &first,
                "Saved Song",
                &[("lyrics", "[00:01]Already supplied")],
            ),
            other,
        ],
    );
    for (name, expected, tagged) in [
        ("blizzard of ozz", vec!["Crazy Train"], 1),
        ("ozzy osbourne", vec!["Crazy Train"], 1),
        ("crazy train", vec!["Crazy Train"], 0),
        ("nobody here", Vec::new(), 0),
    ] {
        let (targets, skipped) = survey(
            &catalog,
            &[name.into()],
            &crate::commands::fetch::EVERYTHING,
        );
        assert_eq!(
            targets
                .iter()
                .map(|target| target.title.as_str())
                .collect::<Vec<_>>(),
            expected,
            "name {name}"
        );
        assert_eq!(skipped.tagged, tagged);
    }
    let scope =
        crate::commands::fetch::Scope::of(&catalog, &[second.to_string_lossy().into_owned()])
            .unwrap();
    for (name, expected) in [("blizzard of ozz", 0), ("other album", 1)] {
        assert_eq!(survey(&catalog, &[name.into()], &scope).0.len(), expected);
    }
    let sidecar = first.join("Crazy Train.lrc");
    let words = "[00:01]Words retained on disk\n";
    std::fs::write(&sidecar, words).unwrap();
    let (targets, skipped) = survey(
        &catalog,
        &["blizzard of ozz".into()],
        &crate::commands::fetch::EVERYTHING,
    );
    assert!(targets.is_empty());
    assert_eq!(skipped.sidecar, 1);
    assert_eq!(skipped.tagged, 1);
    assert_eq!(std::fs::read_to_string(sidecar).unwrap(), words);
    let _ = std::fs::remove_dir_all(dir);
}
