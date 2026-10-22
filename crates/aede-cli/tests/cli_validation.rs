//! Public parsing, operation modes and terminal diagnostics retain strict input.

#[path = "annotation_command_support/mod.rs"]
#[allow(dead_code)]
mod support;

use std::path::Path;
use std::process::{Command, Output, Stdio};

use aede_core::model::Catalog;
#[cfg(feature = "fetch")]
use aede_core::model::EntityKind;
#[cfg(feature = "fetch")]
use aede_core::user::EntityRef;
use aede_core::user::{self, LOCAL_USER, SameArtist};
use aede_core::{spectrum, store};
use support::PersonalLibrary;

fn run(data: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_aede"))
        .args(arguments)
        .env("AEDE_HOME", data)
        .env("NO_COLOR", "1")
        .env_remove("AEDE_DELEGATED_CHILD")
        .env_remove("AEDE_DELEGATED_DATA_DIR")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn snapshots(data: &Path) -> [Option<Vec<u8>>; 6] {
    [
        "catalog.json",
        "conclusions.json",
        "sources.json",
        "user.json",
        "accounts.json",
        "parental.json",
    ]
    .map(|name| std::fs::read(data.join(name)).ok())
}

fn unsafe_controls(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\t'))
}

#[cfg(feature = "fetch")]
#[test]
fn exact_artist_summary_selectors_reach_the_handler_in_both_value_syntaxes() {
    let library = PersonalLibrary::new();
    let mut catalog = library.catalog.clone();
    let mut other = catalog.artists[0].clone();
    other.id += 1;
    other.key = "artist ensemble".into();
    other.name = "Artist Ensemble".into();
    catalog.artists.push(other);
    store::save(&catalog, &store::catalog_path(&library.dir)).unwrap();
    let before = snapshots(&library.dir);
    let reference = EntityRef::of(&catalog, EntityKind::Artist, 1)
        .unwrap()
        .to_token();
    let inline = format!("--artist-ref={reference}");
    let mut failures = Vec::new();
    for arguments in [
        vec!["fetch", "--summaries", &inline, "--dry-run"],
        vec![
            "fetch",
            "--summaries",
            "--artist-ref",
            &reference,
            "--dry-run",
        ],
    ] {
        let result = run(&library.dir, &arguments);
        let output = String::from_utf8(result.stdout).unwrap();
        let error = String::from_utf8(result.stderr).unwrap();
        if !result.status.success() || !output.contains("nothing here matches artist ensemble") {
            failures.push(format!(
                "{arguments:?}: {:?}, {output:?}, {error:?}",
                result.status
            ));
        }
        assert_eq!(snapshots(&library.dir), before, "preview changed stores");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(feature = "fetch")]
#[test]
fn artist_summary_selector_refusals_name_missing_or_invalid_references() {
    let library = PersonalLibrary::new();
    let before = snapshots(&library.dir);
    for (arguments, message) in [
        (
            vec!["fetch", "--summaries", "--artist-ref", "--dry-run"],
            "expects a value",
        ),
        (
            vec![
                "fetch",
                "--summaries",
                "--artist-ref=artist:absent",
                "--dry-run",
            ],
            "absent or ambiguous",
        ),
        (
            vec![
                "fetch",
                "--summaries",
                "--artist-ref=release:wrong",
                "--dry-run",
            ],
            "requires an artist reference",
        ),
    ] {
        let result = run(&library.dir, &arguments);
        assert!(!result.status.success(), "accepted {arguments:?}");
        let error = String::from_utf8(result.stderr).unwrap();
        assert!(error.contains(message), "{arguments:?}: {error}");
        assert_eq!(snapshots(&library.dir), before);
    }
}

#[test]
fn full_decode_refuses_worker_options_it_cannot_honor_without_touching_stores() {
    let library = PersonalLibrary::new();
    store::save(&Catalog::default(), &store::catalog_path(&library.dir)).unwrap();
    let before = snapshots(&library.dir);
    let mut accepted = Vec::new();
    for option in ["--threads=invalid", "--threads=-1", "--threads=1"] {
        for json in [false, true] {
            let mut arguments = vec!["check", "--decode-test", option];
            if json {
                arguments.push("--json");
            }
            let result = run(&library.dir, &arguments);
            let error = String::from_utf8(result.stderr).unwrap();
            if result.status.success() || !error.contains("threads") {
                accepted.push(format!("{arguments:?}: {:?}, {error:?}", result.status));
            }
            assert_eq!(snapshots(&library.dir), before);
        }
    }
    assert!(accepted.is_empty(), "{}", accepted.join("\n"));
}

#[test]
fn spectrum_refuses_invalid_workers_for_empty_current_and_preview_selections() {
    let mut failures = Vec::new();
    for mode in ["empty", "current", "preview"] {
        let library = PersonalLibrary::new();
        let audio = library.dir.join("track.flac");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
            &audio,
        )
        .unwrap();
        let audio_before = std::fs::read(&audio).unwrap();
        let mut catalog = library.catalog.clone();
        catalog.files[0].path = audio.to_str().unwrap().into();
        if mode == "empty" {
            catalog = Catalog::default();
        }
        let picture = spectrum::picture_for(&audio);
        if mode == "current" {
            std::fs::create_dir_all(picture.parent().unwrap()).unwrap();
            std::fs::write(&picture, b"an existing picture must not be replaced").unwrap();
            assert!(!spectrum::out_of_date(&audio, &picture));
        }
        let picture_before = std::fs::read(&picture).ok();
        store::save(&catalog, &store::catalog_path(&library.dir)).unwrap();
        let before = snapshots(&library.dir);
        for option in ["--threads=invalid", "--threads=-1", "--threads=1.5"] {
            let mut arguments = vec!["spectrum", option];
            if mode == "preview" {
                arguments.push("--dry-run");
            }
            let result = run(&library.dir, &arguments);
            let error = String::from_utf8(result.stderr).unwrap();
            if result.status.success() || !error.contains("threads") {
                failures.push(format!(
                    "{mode} {arguments:?}: {:?}, {error:?}",
                    result.status
                ));
            }
            assert_eq!(snapshots(&library.dir), before);
            assert_eq!(std::fs::read(&audio).unwrap(), audio_before);
            assert_eq!(std::fs::read(&picture).ok(), picture_before);
        }
        if mode == "preview" {
            let result = run(&library.dir, &["spectrum", "--threads=1", "--dry-run"]);
            assert!(result.status.success(), "valid preview: {result:?}");
            assert_eq!(std::fs::read(&picture).ok(), picture_before);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn argument_diagnostics_show_terminal_instructions_literally() {
    let library = PersonalLibrary::new();
    let controls = "literal\u{1b}[2J\u{7}\r\u{85}";
    let option = format!("--{controls}");
    let before = snapshots(&library.dir);
    let mut failures = Vec::new();
    for arguments in [
        vec![controls],
        vec!["stats", &option],
        vec![controls, "--json"],
        vec!["stats", controls],
        vec!["album", "--role", controls],
        vec!["--data", controls],
    ] {
        let result = run(&library.dir, &arguments);
        assert_eq!(result.status.code(), Some(2), "{arguments:?}");
        assert!(result.stdout.is_empty(), "{arguments:?}: {result:?}");
        let error = String::from_utf8(result.stderr).unwrap();
        if unsafe_controls(&error) || !error.contains("literal\\u{1b}[2J\\u{7}\\r\\u{85}") {
            failures.push(format!("{arguments:?}: {error:?}"));
        }
        assert_eq!(snapshots(&library.dir), before);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(unix)]
#[test]
fn reset_and_missing_data_diagnostics_escape_the_data_path_and_preserve_the_catalog() {
    let library = PersonalLibrary::new();
    let data = library
        .dir
        .join("literal\u{1b}[2J\u{7}\r\u{1b}[31mred\u{1b}[0m");
    std::fs::create_dir(&data).unwrap();
    store::save(&library.catalog, &store::catalog_path(&data)).unwrap();
    let before = snapshots(&data);
    let mut failures = Vec::new();
    for arguments in [vec!["reset"], vec!["--data"]] {
        let result = run(&data, &arguments);
        assert!(!result.status.success(), "{arguments:?}: {result:?}");
        let output = String::from_utf8(result.stdout).unwrap();
        let error = String::from_utf8(result.stderr).unwrap();
        let combined = format!("{output}{error}");
        if unsafe_controls(&combined)
            || !combined.contains("literal\\u{1b}[2J\\u{7}\\r\\u{1b}[31mred\\u{1b}[0m")
        {
            failures.push(format!("{arguments:?}: {combined:?}"));
        }
        assert_eq!(snapshots(&data), before);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(unix)]
#[test]
fn root_changes_escape_displayed_paths_without_rewriting_the_path_or_audio() {
    let library = PersonalLibrary::new();
    let music = library.dir.join("literal\u{1b}]0;title\u{7}\r");
    std::fs::create_dir(&music).unwrap();
    let audio = music.join("track.flac");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        &audio,
    )
    .unwrap();
    let audio_before = std::fs::read(&audio).unwrap();
    let literal_path = music.canonicalize().unwrap().to_str().unwrap().to_owned();
    let mut catalog = library.catalog.clone();
    catalog.roots = vec![literal_path.clone()];
    store::save(&catalog, &store::catalog_path(&library.dir)).unwrap();
    assert_eq!(
        store::load(&store::catalog_path(&library.dir))
            .unwrap()
            .unwrap()
            .roots
            .as_slice(),
        std::slice::from_ref(&literal_path)
    );
    let mut failures = Vec::new();
    for arguments in [
        vec!["roots", "--exclude", &literal_path, "--no-scan"],
        vec!["roots", "--exclude", &literal_path, "--remove", "--no-scan"],
        vec!["roots", "--remove", &literal_path, "--no-scan"],
    ] {
        let result = run(&library.dir, &arguments);
        assert!(result.status.success(), "{arguments:?}: {result:?}");
        let output = String::from_utf8(result.stdout).unwrap();
        if unsafe_controls(&output) || !output.contains("literal\\u{1b}]0;title\\u{7}\\r") {
            failures.push(format!("{arguments:?}: {output:?}"));
        }
        let saved = store::load(&store::catalog_path(&library.dir))
            .unwrap()
            .unwrap();
        if arguments.contains(&"--exclude") && !arguments.contains(&"--remove") {
            assert_eq!(
                saved.excluded.as_slice(),
                std::slice::from_ref(&literal_path)
            );
        } else {
            assert!(saved.excluded.is_empty());
        }
        if arguments.contains(&"--exclude") {
            assert_eq!(saved.roots.as_slice(), std::slice::from_ref(&literal_path));
        } else {
            assert!(saved.roots.is_empty());
        }
        assert_eq!(std::fs::read(&audio).unwrap(), audio_before);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(unix)]
#[test]
fn scan_roots_check_and_spectrum_preserve_known_style_tokens_in_literal_paths() {
    let library = PersonalLibrary::new();
    let data = library.dir.join("dataliteral\u{1b}[31mred\u{1b}[0m");
    let music = library.dir.join("watchedliteral\u{1b}[31mred\u{1b}[0m");
    let excluded = music.join("excludedliteral\u{1b}[31mred\u{1b}[0m");
    std::fs::create_dir(&data).unwrap();
    std::fs::create_dir_all(&excluded).unwrap();
    std::fs::copy(user::user_path(&library.dir), user::user_path(&data)).unwrap();
    let personal_before = std::fs::read(user::user_path(&data)).unwrap();
    let audio = music.join("track.flac");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        &audio,
    )
    .unwrap();
    let audio_before = std::fs::read(&audio).unwrap();
    let music = music.canonicalize().unwrap();
    let excluded = excluded.canonicalize().unwrap();
    let audio = audio.canonicalize().unwrap();
    let catalog_path = store::catalog_path(&data);
    let mut failures = Vec::new();
    let mut literal_line = |output: &str, boundary: &str, marker: &str, context: &str| {
        let expected = format!("{marker}\\u{{1b}}[31mred\\u{{1b}}[0m");
        if !output.lines().any(|line| {
            line.contains(context) && line.contains(&expected) && !unsafe_controls(line)
        }) {
            failures.push(format!(
                "{boundary} did not preserve {expected:?}: {output:?}"
            ));
        }
    };

    let result = run(&data, &["scan", music.to_str().unwrap()]);
    assert!(result.status.success(), "scan failed: {result:?}");
    let output = String::from_utf8(result.stdout).unwrap();
    literal_line(&output, "scan catalog footer", "dataliteral", "catalog:");
    let mut catalog = store::load(&catalog_path).unwrap().unwrap();
    assert_eq!(
        catalog.roots.iter().map(Path::new).collect::<Vec<_>>(),
        [&music]
    );
    assert_eq!(Path::new(&catalog.files[0].path), audio);
    let integrity = aede_core::audit::integrity::check(&audio).unwrap();
    catalog.files[0].integrity = Some(aede_core::model::IntegrityRecord {
        verdict: integrity.verdict,
        method: integrity.method.into(),
        checked_at: 1,
    });
    catalog.excluded = vec![excluded.to_str().unwrap().into()];
    store::save(&catalog, &catalog_path).unwrap();
    let before = snapshots(&data);

    for arguments in [
        vec!["roots"],
        vec!["check", music.to_str().unwrap()],
        vec!["spectrum", "--dry-run"],
    ] {
        let result = run(&data, &arguments);
        assert!(result.status.success(), "{arguments:?}: {result:?}");
        let output = String::from_utf8(result.stdout).unwrap();
        match arguments[0] {
            "roots" => {
                let (watched_table, excluded_table) = output.split_once("Never read").unwrap();
                literal_line(
                    watched_table,
                    "watched-root table",
                    "watchedliteral",
                    "watchedliteral",
                );
                literal_line(
                    excluded_table,
                    "excluded-root table",
                    "excludedliteral",
                    "excludedliteral",
                );
                literal_line(&output, "roots catalog footer", "dataliteral", "catalog:");
            }
            "check" => literal_line(&output, "check scope", "watchedliteral", "  in "),
            "spectrum" => literal_line(
                &output,
                "spectrum picture",
                "watchedliteral",
                "spectrograms",
            ),
            _ => unreachable!(),
        }
        assert_eq!(snapshots(&data), before, "{arguments:?} changed stores");
        let saved = store::load(&catalog_path).unwrap().unwrap();
        assert_eq!(
            saved.roots.iter().map(Path::new).collect::<Vec<_>>(),
            [&music]
        );
        assert_eq!(
            saved.excluded.iter().map(Path::new).collect::<Vec<_>>(),
            [&excluded]
        );
        assert_eq!(Path::new(&saved.files[0].path), audio);
        assert_eq!(std::fs::read(&audio).unwrap(), audio_before);
        assert_eq!(
            std::fs::read(user::user_path(&data)).unwrap(),
            personal_before
        );
        assert!(!spectrum::picture_for(&audio).exists());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn stats_refuses_copy_encoding_options_instead_of_printing_an_unfiltered_answer() {
    let library = PersonalLibrary::new();
    let before = snapshots(&library.dir);
    let mut accepted = Vec::new();
    for option in [
        "--compress=flac",
        "--quality=5",
        "--compress=invalid",
        "--quality=invalid",
    ] {
        let result = run(&library.dir, &["stats", option]);
        if result.status.success() || result.stderr.is_empty() {
            accepted.push(option);
        }
        assert_eq!(snapshots(&library.dir), before);
    }
    assert!(accepted.is_empty(), "accepted {accepted:?}");
}

#[test]
fn merge_refuses_an_unsupported_source_in_every_operation_mode() {
    let mut failures = Vec::new();
    for arguments in [
        vec!["merge", "--list", "--source=musicbrainz"],
        vec!["merge", "New Spelling", "Artist", "--source=musicbrainz"],
        vec!["merge", "--forget", "alias", "--source=musicbrainz"],
    ] {
        let library = PersonalLibrary::new();
        let mut personal = library.data();
        personal.same_artist.push(SameArtist {
            owner: LOCAL_USER.into(),
            spelling: "alias".into(),
            filed_as: "artist".into(),
            created_at: 1,
        });
        library.save(&personal);
        let before = snapshots(&library.dir);
        let result = run(&library.dir, &arguments);
        let error = String::from_utf8(result.stderr).unwrap();
        if result.status.success() || !error.contains("source") || snapshots(&library.dir) != before
        {
            failures.push(format!(
                "{arguments:?}: {:?}, {error:?}, stores unchanged={}",
                result.status,
                snapshots(&library.dir) == before
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn merge_refuses_simultaneous_listing_and_removal_without_changing_the_rule() {
    let library = PersonalLibrary::new();
    let mut personal = library.data();
    personal.same_artist.push(SameArtist {
        owner: LOCAL_USER.into(),
        spelling: "alias".into(),
        filed_as: "artist".into(),
        created_at: 1,
    });
    library.save(&personal);
    let before = snapshots(&library.dir);
    let result = run(&library.dir, &["merge", "--list", "--forget", "alias"]);
    assert!(
        !result.status.success(),
        "conflicting operations succeeded: {result:?}"
    );
    assert_eq!(snapshots(&library.dir), before);
    assert!(
        run(&library.dir, &["merge", "--list", "alias"])
            .status
            .success()
    );
}

#[test]
fn missing_refuses_invalid_list_windows_and_removal_without_forget() {
    let library = PersonalLibrary::new();
    let mut personal = library.data();
    personal.set_aside.push(user::SetAside {
        owner: LOCAL_USER.into(),
        release_group: "11111111-1111-4111-8111-111111111111".into(),
        title: "An absent album".into(),
        created_at: 1,
    });
    library.save(&personal);
    let before = snapshots(&library.dir);
    let mut failures = Vec::new();
    for arguments in [
        vec!["missing", "--list", "--limit=invalid"],
        vec!["missing", "--list", "--offset=invalid"],
        vec!["missing", "--remove", "An absent album"],
    ] {
        let result = run(&library.dir, &arguments);
        if result.status.success() || result.stderr.is_empty() {
            failures.push(format!("{arguments:?}: {:?}", result.status));
        }
        assert_eq!(snapshots(&library.dir), before);
    }
    assert!(run(&library.dir, &["missing", "--list"]).status.success());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[cfg(unix)]
#[test]
fn recursive_import_reports_unreadable_folders_before_publishing_and_retries_readable_inputs() {
    use std::os::unix::fs::PermissionsExt;

    let library = PersonalLibrary::new();
    let prior = aede_core::analysis::FileAnalysis {
        path: library.dir.join("previous.flac").to_str().unwrap().into(),
        source: "flaccompagnon".into(),
        source_version: 1,
        imported_at: 1,
        result_at_ns: 1,
        real_bit_depth: Some(20),
        ..Default::default()
    };
    let catalog_path = store::catalog_path(&library.dir);
    let mut catalog = library.catalog.clone();
    catalog.analyses.push(prior.clone());
    store::save(&catalog, &catalog_path).unwrap();
    let before = snapshots(&library.dir);
    let personal_before = library.bytes();
    let reports = library.dir.join("reports");
    let blocked = reports.join("unreadable-reports");
    std::fs::create_dir_all(&blocked).unwrap();
    let good_path = library.dir.join("good.flac");
    let other_path = library.dir.join("other.flac");
    for (destination, analysed, depth) in [
        (reports.join("good.json"), &good_path, 16),
        (blocked.join("other.json"), &other_path, 24),
    ] {
        let path =
            aede_core::json::Json::Str(analysed.to_str().unwrap().into()).to_string_compact();
        let report = format!(
            "{{\"format\":\"flaccompagnon-report\",\"version\":1,\"report\":{{\"files\":[{{\"path\":{path},\"real_bit_depth\":{depth}}}]}}}}"
        );
        std::fs::write(&destination, report).unwrap();
        assert_eq!(
            aede_core::analysis::read_report(&destination)
                .unwrap()
                .files
                .len(),
            1,
            "the traversal fixture must hold a valid report"
        );
    }

    let permissions = std::fs::metadata(&blocked).unwrap().permissions();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let permission_error = std::fs::read_dir(&blocked).err();
    let attempt = Command::new(env!("CARGO_BIN_EXE_aede"))
        .arg("import")
        .arg(&reports)
        .env("AEDE_HOME", &library.dir)
        .env("NO_COLOR", "1")
        .env_remove("AEDE_DELEGATED_CHILD")
        .env_remove("AEDE_DELEGATED_DATA_DIR")
        .stdin(Stdio::null())
        .output();
    std::fs::set_permissions(&blocked, permissions).unwrap();
    let result = attempt.unwrap();
    let output = String::from_utf8(result.stdout).unwrap();
    let error = String::from_utf8(result.stderr).unwrap();
    let mut failures = Vec::new();
    if let Some(permission_error) = permission_error {
        assert_eq!(
            permission_error.kind(),
            std::io::ErrorKind::PermissionDenied
        );
        eprintln!("PermissionDenied branch exercised: mode-000 report directory was unreadable");
        if result.status.success() {
            failures.push(format!("unreadable import succeeded: {output:?}"));
        }
        if !error.contains(blocked.to_str().unwrap()) {
            failures.push(format!("offending directory was not reported: {error:?}"));
        }
        if output.contains("Reports read") || output.contains("Analyses stored") {
            failures.push(format!(
                "an incomplete import printed its success summary: {output:?}"
            ));
        }
        if snapshots(&library.dir) != before {
            failures.push("an incomplete import changed persistent stores".into());
        }
    } else {
        eprintln!(
            "Privileged Unix branch exercised: mode-000 directory remained readable; refusal coverage was not exercised"
        );
        assert!(result.status.success(), "{output:?}, {error:?}");
        let imported = store::load(&catalog_path).unwrap().unwrap();
        for path in [&good_path, &other_path] {
            assert!(
                imported
                    .analyses
                    .iter()
                    .any(|record| Path::new(&record.path) == path),
                "the readable directory must import both reports"
            );
        }
    }

    let retry = run(&library.dir, &["import", reports.to_str().unwrap()]);
    assert!(retry.status.success(), "readable retry failed: {retry:?}");
    let imported = store::load(&catalog_path).unwrap().unwrap();
    assert_eq!(imported.analyses.len(), 3);
    assert!(imported.analyses.contains(&prior));
    for (path, depth) in [(&good_path, 16), (&other_path, 24)] {
        assert!(imported.analyses.iter().any(|record| {
            Path::new(&record.path) == path
                && record.source == "flaccompagnon"
                && record.real_bit_depth == Some(depth)
        }));
    }
    assert_eq!(library.bytes(), personal_before);
    assert_eq!(imported.files.len(), library.catalog.files.len());
    assert_eq!(imported.roots, library.catalog.roots);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
