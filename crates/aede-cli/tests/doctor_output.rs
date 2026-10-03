//! Local diagnostic output keeps its pagination and literal display contracts.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use aede_core::model::{self, ScannedFile};
use aede_core::{json, store, tags};

struct Library(PathBuf);

impl Library {
    fn new(files: Vec<ScannedFile>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "aede-doctor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        store::save(
            &model::build(files, vec!["/music".into()], 1, &[]),
            &store::catalog_path(&directory),
        )
        .unwrap();
        Self(directory)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
        command
            .arg("doctor")
            .arg("--data")
            .arg(&self.0)
            .args(args)
            .env("NO_COLOR", "1");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn text(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn track(album: &str) -> ScannedFile {
    let mut tags = tags::RawTags::default();
    for (key, value) in [
        ("album", album),
        ("artist", "A"),
        ("title", album),
        ("tracknumber", "1"),
        ("date", "2020"),
    ] {
        tags.insert(key, value);
    }
    tags.properties.codec = "flac".into();
    tags.properties.lossless = true;
    tags.properties.duration_ms = Some(1000);
    ScannedFile {
        path: format!("/music/{album}/01.flac"),
        size: 1,
        mtime: 1,
        tags,
        folder_cover: None,
        sidecar: None,
        integrity: None,
        fingerprint: None,
    }
}

#[test]
fn severity_filter_and_pagination_select_the_same_human_and_json_issues() {
    let library = Library::new(["Alpha", "Beta", "Gamma"].map(track).to_vec());
    let full = json::parse(&library.text(&["--severity=info", "--all", "--json"])).unwrap();
    let page =
        json::parse(&library.text(&["--severity=info", "--limit=1", "--offset=1", "--json"]))
            .unwrap();
    let rows = page.as_arr().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0], full.as_arr().unwrap()[1]);
    let human = library.text(&["--severity=info", "--limit=1", "--offset=1"]);
    assert!(human.contains("Beta"), "{human}");
    assert!(
        !human.contains("Alpha") && !human.contains("Gamma"),
        "{human}"
    );
    let beyond = json::parse(&library.text(&["--offset=100", "--json"])).unwrap();
    assert!(beyond.as_arr().unwrap().is_empty());
}

#[test]
fn invalid_windows_are_refused_even_for_json_or_an_empty_severity() {
    let library = Library::new(vec![track("Album")]);
    for args in [
        vec!["--limit=bad", "--json"],
        vec!["--all", "--limit=2", "--json"],
        vec!["--severity=error", "--offset=bad"],
    ] {
        let output = library.run(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "invalid windows must be refused before any report is printed"
        );
    }
}

#[test]
fn terminal_control_bytes_in_details_and_paths_are_shown_literally() {
    let album = "Album\u{1b}[31m\rInjected";
    let library = Library::new(vec![track(album)]);
    let human = library.text(&["--all"]);
    assert!(
        !human.contains('\u{1b}') && !human.contains('\r'),
        "{human:?}"
    );
    let machine = json::parse(&library.text(&["--all", "--json"])).unwrap();
    assert!(machine.as_arr().unwrap().iter().any(|issue| {
        issue
            .get("detail")
            .and_then(|detail| detail.as_str())
            .is_some_and(|detail| detail.contains(album))
    }));
}

#[test]
fn extreme_announced_totals_finish_without_enumerating_billions_of_missing_positions() {
    let mut scanned = track("Large totals");
    scanned.tags.insert("tracktotal", u32::MAX.to_string());
    scanned.tags.insert("disctotal", u32::MAX.to_string());
    let library = Library::new(vec![scanned]);
    let mut child = library
        .command(&["--all"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let timed_out = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            break true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    assert!(
        !timed_out,
        "untrusted total tags must not trigger an unbounded diagnostic walk"
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let human = String::from_utf8(output.stdout).unwrap();
    assert!(
        human.contains("missing tracks 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13…"),
        "{human}"
    );
    assert!(
        human.contains("missing discs 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13… of 4294967295"),
        "{human}"
    );
}
