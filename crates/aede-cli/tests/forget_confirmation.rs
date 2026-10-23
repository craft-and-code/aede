//! Explicit consent is required before persistent analysis/source deletion.

#[path = "forget_confirmation_support/mod.rs"]
mod support;

use aede_core::sources;
use std::io::Write;
use std::process::Stdio;
use support::Fixture;

#[test]
fn noninteractive_forgetting_refuses_even_piped_yes_and_preserves_all_stores() {
    for arguments in [
        vec!["import", "--forget"],
        vec!["import", "--forget", "--pending"],
        vec!["import", "--forget", "--source=flaccompagnon"],
        vec!["sources", "--forget"],
        vec!["sources", "--forget", "--source=musicbrainz"],
    ] {
        let fixture = Fixture::new();
        let before = fixture.snapshot();
        for piped in [false, true] {
            let output = if piped {
                let mut child = fixture
                    .command(&arguments)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(b"yes\n")
                    .unwrap_or_else(|error| {
                        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe)
                    });
                child.wait_with_output().unwrap()
            } else {
                fixture.run(&arguments)
            };
            assert_eq!(output.status.code(), Some(1), "{arguments:?}: {output:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains("--yes"));
            let preview = String::from_utf8(output.stdout).unwrap();
            assert!(
                preview.contains(fixture.data().canonicalize().unwrap().to_str().unwrap()),
                "{preview}"
            );
            assert!(preview.contains("to remove"), "{preview}");
            assert!(!preview.contains("Type \"yes\""), "pipe must not prompt");
            assert_eq!(fixture.snapshot(), before, "{arguments:?}");
        }
    }
}

#[test]
fn explicit_yes_removes_only_the_selected_analyses_or_source_decisions() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    let folder = fixture.library.music.to_str().unwrap();
    let output = fixture.run(&[
        "import",
        "--forget",
        "--pending",
        folder,
        "--source=flaccompagnon",
        "--yes",
    ]);
    assert!(output.status.success(), "{output:?}");
    let kept = fixture.catalog();
    assert_eq!(kept.analyses.len(), 2);
    assert!(
        kept.analyses
            .iter()
            .any(|a| a.path == fixture.library.audio.to_str().unwrap())
    );
    assert!(kept.analyses.iter().any(|a| a.source == "another-tool"));
    assert_eq!(fixture.snapshot()[2..], before[2..]);

    let before = fixture.snapshot();
    let output = fixture.run(&["sources", "--forget", "--source=musicbrainz", "--yes"]);
    assert!(output.status.success(), "{output:?}");
    let kept = fixture.sources();
    assert_eq!(kept.records.len(), 1);
    assert_eq!(kept.records[0].source, "another-provider");
    assert_eq!(kept.reviews.len(), 1);
    assert_eq!(kept.reviews[0].source, "another-provider");
    assert!(kept.credit_exclusions.is_empty());
    let after = fixture.snapshot();
    assert_eq!(after[..2], before[..2]);
    assert_eq!(after[3..], before[3..]);
    assert_eq!(
        std::fs::read(&fixture.library.audio).unwrap(),
        b"unchanged audio sentinel"
    );
}

#[test]
fn decisions_without_cached_source_records_still_require_confirmation() {
    let fixture = Fixture::new();
    let mut held = fixture.sources();
    held.records.clear();
    sources::save(&held, &sources::sources_path(fixture.data())).unwrap();
    let before = fixture.snapshot();
    let output = fixture.run(&["sources", "--forget"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(fixture.snapshot(), before);
    let output = fixture.run(&["sources", "--forget", "--yes"]);
    assert!(output.status.success(), "{output:?}");
    assert!(fixture.sources().reviews.is_empty());
    assert!(fixture.sources().credit_exclusions.is_empty());
}

#[test]
fn empty_forget_operations_do_not_prompt_or_rewrite_stores() {
    let fixture = Fixture::new();
    sources::save(&Default::default(), &sources::sources_path(fixture.data())).unwrap();
    for entry in std::fs::read_dir(fixture.data()).unwrap() {
        std::fs::File::options()
            .write(true)
            .open(entry.unwrap().path())
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(100)),
            )
            .unwrap();
    }
    let before = fixture.snapshot();
    for arguments in [
        vec!["import", "--forget", "--source=absent"],
        vec!["sources", "--forget"],
    ] {
        let output = fixture.run(&arguments);
        assert!(output.status.success(), "{arguments:?}: {output:?}");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("Type \"yes\""));
        assert_eq!(fixture.snapshot(), before, "no-op rewrote stores");
    }
}

#[test]
fn confirmation_uses_the_explicit_data_directory_instead_of_the_environment_default() {
    for command in ["import", "sources"] {
        let original = Fixture::new();
        let selected = Fixture::new();
        let before = original.snapshot();
        let output = original.run(&[
            command,
            "--forget",
            "--data",
            selected.data().to_str().unwrap(),
            "--yes",
        ]);
        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains(selected.data().canonicalize().unwrap().to_str().unwrap())
        );
        assert_eq!(original.snapshot(), before);
        if command == "import" {
            assert!(selected.catalog().analyses.is_empty());
        } else {
            assert!(selected.sources().records.is_empty());
        }
    }
}

#[test]
fn yes_without_forget_and_invalid_requests_do_not_change_data() {
    let fixture = Fixture::new();
    let before = fixture.snapshot();
    for arguments in [
        vec!["import", "--list", "--yes"],
        vec!["import", "--pending", "--yes"],
        vec!["sources", "--yes"],
        vec!["sources", "--export", "--yes"],
        vec!["sources", "--template", "--yes"],
        vec!["sources", "--import=missing.json", "--yes"],
        vec!["sources", "--forget", "--source=absent", "--yes"],
        vec!["import", "--forget", "--list", "--yes"],
        vec!["sources", "--forget", "--list", "--yes"],
        vec!["import", "--forget", "--yes=false"],
    ] {
        assert!(!fixture.run(&arguments).status.success(), "{arguments:?}");
        assert_eq!(fixture.snapshot(), before, "{arguments:?}");
    }
    std::fs::write(
        sources::sources_path(fixture.data()),
        b"corrupt source store",
    )
    .unwrap();
    let before = fixture.snapshot();
    assert!(
        !fixture
            .run(&["sources", "--forget", "--yes"])
            .status
            .success()
    );
    assert_eq!(fixture.snapshot(), before);
}

#[cfg(unix)]
#[test]
fn terminal_confirmation_requires_yes_and_preserves_data_until_the_answer() {
    for command in ["import", "sources"] {
        for answer in ["yes", "no", "", "eof"] {
            let fixture = Fixture::new();
            let before = fixture.snapshot();
            let result = fixture.terminal(command, answer);
            assert_eq!(result.field_u64("code"), Some(0), "{result:?}");
            assert!(result.field_bool("unchanged_before_confirmation"));
            if answer == "yes" {
                if command == "import" {
                    assert!(fixture.catalog().analyses.is_empty());
                } else {
                    assert!(fixture.sources().records.is_empty());
                    assert!(fixture.sources().reviews.is_empty());
                }
            } else {
                assert!(
                    result
                        .field_str("stdout")
                        .unwrap()
                        .contains("nothing was removed")
                );
                assert_eq!(fixture.snapshot(), before);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn delegated_forgetting_obeys_confirmation_and_disconnect_never_consents() {
    for command in ["import", "sources"] {
        let fixture = Fixture::new();
        let _server = support::Server::start(&fixture);
        let before = fixture.snapshot();
        assert_eq!(fixture.run(&[command, "--forget"]).status.code(), Some(1));
        assert_eq!(fixture.snapshot(), before);
        for answer in ["no", "disconnect"] {
            let _result = fixture.terminal(command, answer);
            // The next writer is a deterministic fence for the disconnected child.
            assert!(
                fixture
                    .run(&["import", "--forget", "--source=absent", "--yes"])
                    .status
                    .success()
            );
            assert_eq!(fixture.snapshot(), before);
        }
        let result = fixture.terminal(command, "yes");
        assert_eq!(result.field_u64("code"), Some(0));
        if command == "import" {
            assert!(fixture.catalog().analyses.is_empty());
        } else {
            assert!(fixture.sources().records.is_empty());
        }
    }
}
