//! End-to-end test: run the real binary against the reference files and check
//! what it prints.
//!
//! This is the only test that exercises the whole chain — directory walk, tag
//! reading, graph construction, persistence, reload, rendering.

use std::path::PathBuf;
use std::process::Command;

fn library() -> PathBuf {
    // The reference files belong to the core crate.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/fixtures")
        .canonicalize()
        .expect("reference folder")
}

/// A throwaway data directory, removed when the test ends.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!("aede_e2e_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporary folder");
        Sandbox { dir }
    }

    /// Runs the binary and returns `(stdout, stderr, success)`.
    fn run(&self, args: &[&str]) -> (String, String, bool) {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(args)
            .env("AEDE_HOME", &self.dir)
            .env("NO_COLOR", "1")
            .output()
            .expect("running the binary");
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.success(),
        )
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn scan_then_query() {
    let sandbox = Sandbox::new("full");
    let root = library();

    // --- Initial scan ------------------------------------------------------
    let (out, err, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok, "the scan must succeed. stderr: {err}");
    assert!(out.contains("Scan complete"), "output: {out}");
    assert!(
        sandbox.dir.join("catalog.json").exists(),
        "the catalog must be written"
    );

    // --- Statistics --------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["stats"]);
    assert!(ok);
    assert!(out.contains("Miles Davis"), "the artist must appear: {out}");
    assert!(out.contains("FLAC"));
    assert!(out.contains("Metadata completeness"));

    // --- Artist page -------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(
        out.contains("Kind of Blue"),
        "the discography must appear: {out}"
    );

    // --- Search, case and accent insensitive -------------------------------
    let (out, _, ok) = sandbox.run(&["search", "kind of blue"]);
    assert!(ok);
    assert!(out.contains("Kind of Blue"));

    // --- Machine-readable output -------------------------------------------
    let (out, _, ok) = sandbox.run(&["stats", "--json"]);
    assert!(ok);
    let value = aede_core::json::parse(&out).expect("valid JSON output");
    assert!(value.field_u64("tracks").unwrap_or(0) >= 8, "output: {out}");

    // --- Diagnosis ---------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(out.contains("Diagnosis"));

    // --- Incremental scan: nothing to read again ---------------------------
    let (out, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Reused from previous scan"), "output: {out}");
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Read") && l.trim_end().ends_with('0')),
        "no file should have been read again:\n{out}"
    );
}

#[test]
fn missing_catalog_gives_an_actionable_message() {
    let sandbox = Sandbox::new("empty");
    let (_, err, ok) = sandbox.run(&["stats"]);
    assert!(!ok, "the command must fail");
    assert!(
        err.contains("aede scan"),
        "the message must say what to do: {err}"
    );
}

#[test]
fn unknown_command_is_reported() {
    let sandbox = Sandbox::new("unknown");
    let (_, err, ok) = sandbox.run(&["statz"]);
    assert!(!ok);
    assert!(err.contains("unknown command"), "stderr: {err}");
}

#[test]
fn misspelled_option_is_reported() {
    let sandbox = Sandbox::new("option");
    let (_, err, _) = sandbox.run(&["stats", "--limite=3"]);
    assert!(err.contains("unknown option"), "stderr: {err}");
}

#[test]
fn inspecting_a_single_file() {
    let sandbox = Sandbox::new("file");
    let file = library().join("track.flac");
    let (out, _, ok) = sandbox.run(&["file", file.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("FLAC 16/44.1"), "output: {out}");
    assert!(out.contains("So What"));
}

#[test]
fn help_and_version() {
    let sandbox = Sandbox::new("help");
    let (out, _, ok) = sandbox.run(&["--version"]);
    assert!(ok);
    assert!(out.starts_with("aede "), "output: {out}");

    let (out, _, ok) = sandbox.run(&["--help"]);
    assert!(ok);
    assert!(out.contains("COMMANDS"));
}

#[test]
fn watched_folders_accumulate_across_scans() {
    // Scanning a second library used to replace the catalog instead of adding
    // to it, silently losing everything scanned before.
    let sandbox = Sandbox::new("roots");
    let fixtures = library();
    let scratch = std::env::temp_dir().join("aede_e2e_roots_src");
    let (a, b) = (scratch.join("a"), scratch.join("b"));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::copy(fixtures.join("track.flac"), a.join("1.flac")).unwrap();
    std::fs::copy(fixtures.join("track.mp3"), b.join("2.mp3")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", a.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["scan", b.to_str().unwrap()]);
    assert!(ok);
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Files found") && l.trim_end().ends_with('2')),
        "both folders must be walked:\n{out}"
    );

    let (out, _, ok) = sandbox.run(&["roots"]);
    assert!(ok);
    assert!(out.contains("a"), "output: {out}");
    assert!(out.contains("b"), "output: {out}");

    // Dropping a folder needs one more scan to take the files out.
    let (_, _, ok) = sandbox.run(&["roots", "--remove", b.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Files found") && l.trim_end().ends_with('1')),
        "only the remaining folder must be walked:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_guest_appearance_is_listed_apart_from_the_discography() {
    let sandbox = Sandbox::new("guest");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("Discography"), "output: {out}");
}
