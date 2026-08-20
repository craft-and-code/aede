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

#[test]
fn a_track_is_reachable_by_its_title() {
    // The point of the command: the same page as `file`, without having to
    // type a path.
    let sandbox = Sandbox::new("track");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(out.contains("Kind of Blue"), "the album is shown: {out}");
    assert!(out.contains("Miles Davis"), "the credits are shown: {out}");
    assert!(out.contains("Sample rate"), "the technical panel is shown");

    // Several files carry that title: all of them are printed.
    let pages = out.matches("Album artist").count();
    assert!(pages > 1, "every match is shown, got {pages}");

    // A limit must be announced, never silent.
    let (out, _, ok) = sandbox.run(&["track", "So What", "--limit=1"]);
    assert!(ok);
    assert_eq!(out.matches("Album artist").count(), 1);
    assert!(out.contains("shown"), "the truncation is announced: {out}");

    // An unknown title fails with a usable message.
    let (_, err, ok) = sandbox.run(&["track", "no such title here"]);
    assert!(!ok);
    assert!(err.contains("no track matches"), "stderr: {err}");

    // A filter that excludes everything says so rather than denying the title.
    let (_, err, ok) = sandbox.run(&["track", "So What", "--artist=Ozzy Osbourne"]);
    assert!(!ok);
    assert!(err.contains("none matching the filters"), "stderr: {err}");
}

#[test]
fn dropping_the_last_folder_lets_the_catalog_be_emptied() {
    // `roots --remove` says to run `aede scan` to drop the files. When the
    // folder removed was the only one, that scan used to fail for want of a
    // folder, and the files had no way out of the catalog.
    let sandbox = Sandbox::new("last_root");
    let scratch = std::env::temp_dir().join("aede_e2e_last_root_src");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::copy(library().join("track.flac"), scratch.join("1.flac")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", scratch.to_str().unwrap()]);
    assert!(ok);
    let (_, _, ok) = sandbox.run(&["roots", "--remove", scratch.to_str().unwrap()]);
    assert!(ok);

    let (out, err, ok) = sandbox.run(&["scan"]);
    assert!(ok, "the scan must run with no folder left. stderr: {err}");
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Gone since") && l.trim_end().ends_with('1')),
        "the file must leave the catalog:\n{out}"
    );

    let (out, _, ok) = sandbox.run(&["stats"]);
    assert!(ok);
    assert!(
        out.lines()
            .any(|l| l.trim_start().starts_with("Tracks") && l.trim_end().ends_with('0')),
        "the catalog must be empty:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn a_first_scan_still_demands_a_folder() {
    let sandbox = Sandbox::new("no_catalog");
    let (_, err, ok) = sandbox.run(&["scan"]);
    assert!(!ok, "with no catalog there is nothing to infer");
    assert!(err.contains("give at least one folder"), "stderr: {err}");
}

#[test]
fn every_listing_carries_the_same_measures() {
    // Count, duration and size: what a slice of the library weighs must not
    // depend on the command used to look at it.
    let sandbox = Sandbox::new("measures");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    for command in ["artists", "albums", "genres", "labels", "years"] {
        let (out, _, ok) = sandbox.run(&[command]);
        assert!(ok, "{command} must run");
        assert!(
            out.contains("Duration"),
            "{command} lacks a duration:\n{out}"
        );
        assert!(out.contains("Size"), "{command} lacks a size:\n{out}");
    }

    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(
        out.contains(" KB") || out.contains(" MB") || out.contains(" B"),
        "the artist page must state a size:\n{out}"
    );
}

#[test]
fn a_guest_appearance_is_timed_on_its_own_tracks() {
    // The row used to count one track and time the whole album: a single guest
    // song read as forty minutes of music.
    let sandbox = Sandbox::new("guest_duration");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);

    let appears = out
        .split("Appears on")
        .nth(1)
        .expect("an Appears on section");
    let row = appears
        .lines()
        .find(|l| l.contains("Kind of Blue"))
        .expect("a release row");
    // Nine tracks of one second each: the row cannot claim the whole album.
    assert!(
        row.contains("0:0"),
        "the duration must cover the counted tracks only: {row}"
    );
}

#[test]
fn a_writer_is_not_announced_as_having_nothing() {
    // A band's lyricist has no performing credit at all. The page used to open
    // with "0 album · 0 track" and then list the forty albums he wrote for.
    let sandbox = Sandbox::new("writer");
    let scratch = std::env::temp_dir().join("aede_e2e_writer_src");
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::copy(library().join("track.flac"), scratch.join("1.flac")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", scratch.to_str().unwrap()]);
    assert!(ok);

    // "track.flac" credits Miles Davis as both performer and composer, so the
    // performing line wins and no writing line is printed for him.
    let (out, _, ok) = sandbox.run(&["artist", "Miles Davis"]);
    assert!(ok);
    assert!(out.contains("performing:"), "the line is labelled:\n{out}");
    assert!(
        !out.contains("writing:"),
        "a performed track is not counted twice:\n{out}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn checking_a_library_finds_a_damaged_file() {
    let sandbox = Sandbox::new("check");
    // Deliberately deep: a real library sits far from the root, and on macOS a
    // temporary path alone is sixty columns before the file name starts. The
    // report has to keep the name, which is the only part that identifies the
    // file.
    let root = std::env::temp_dir().join("aede_e2e_check_src");
    let scratch = root.join("a-library-buried-under-a-long-and-tiresome-path/second-level");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&scratch).unwrap();
    std::fs::copy(library().join("track.flac"), scratch.join("good.flac")).unwrap();
    std::fs::copy(library().join("track.mp3"), scratch.join("nothing.mp3")).unwrap();
    let mut bytes = std::fs::read(library().join("track.flac")).unwrap();
    let index = bytes.len() - 200;
    bytes[index] ^= 0x01;
    std::fs::write(scratch.join("bad.flac"), &bytes).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", scratch.to_str().unwrap()]);
    assert!(ok);

    // Before any check, nothing is claimed about the files.
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(out.contains("not verified"), "doctor must say so: {out}");

    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok, "the check must run: {out}");
    assert!(out.contains("Damaged"), "output: {out}");
    assert!(out.contains("bad.flac"), "the file is named: {out}");

    // The verdict is stored: a second run has nothing left to read.
    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    assert!(out.contains("already has a verdict"), "output: {out}");

    // And doctor now reports the damage as an error.
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(out.contains("damaged audio"), "output: {out}");
    assert!(!out.contains("not verified"), "everything was verified");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn checking_can_be_restricted_to_one_folder() {
    // Verifying a whole library is a long job; being able to try it on a corner
    // first is what makes it approachable.
    let sandbox = Sandbox::new("check_scope");
    let root = std::env::temp_dir().join("aede_e2e_scope_src");
    let (left, right) = (root.join("left"), root.join("right"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&left).unwrap();
    std::fs::create_dir_all(&right).unwrap();
    std::fs::copy(library().join("track.flac"), left.join("1.flac")).unwrap();
    std::fs::copy(library().join("hires.flac"), right.join("2.flac")).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["check", left.to_str().unwrap()]);
    assert!(ok, "output: {out}");
    assert!(out.contains("1 file to read"), "output: {out}");

    // The other folder is untouched, and the report says so.
    let (out, _, ok) = sandbox.run(&["check", right.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("1 file to read"), "output: {out}");

    // Everything now has a verdict.
    let (out, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    assert!(out.contains("already has a verdict"), "output: {out}");

    // A folder the catalog knows nothing about is not silently an empty run.
    let (out, _, ok) = sandbox.run(&["check", std::env::temp_dir().to_str().unwrap()]);
    assert!(ok);
    assert!(
        out.contains("verdict") || out.contains("no file"),
        "output: {out}"
    );

    // A folder that does not exist is an error, not an empty result.
    let (_, err, ok) = sandbox.run(&["check", "/nowhere/at/all"]);
    assert!(!ok);
    assert!(err.contains("does not exist"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn exporting_as_csv_and_as_a_playlist() {
    let sandbox = Sandbox::new("export");
    let root = library();
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // --- Albums, one row each ----------------------------------------------
    let (out, _, ok) = sandbox.run(&["export", "--csv"]);
    assert!(ok);
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].starts_with("album_artist,album,year"),
        "header: {}",
        lines[0]
    );
    assert!(
        lines.iter().any(|l| l.contains("Kind of Blue")),
        "the album is there:\n{out}"
    );
    // Raw values: a spreadsheet has to be able to add the column up.
    assert!(
        lines[1]
            .split(',')
            .any(|cell| cell.parse::<u64>().is_ok_and(|n| n > 1000)),
        "sizes and durations are numbers: {}",
        lines[1]
    );
    assert!(out.contains("\r\n"), "RFC 4180 line endings");

    // --- One row per track --------------------------------------------------
    let (out, _, ok) = sandbox.run(&["export", "--csv", "--tracks"]);
    assert!(ok);
    assert!(
        out.lines()
            .next()
            .unwrap()
            .starts_with("artist,album_artist")
    );
    assert!(out.contains("So What"));

    // --- The separator can be changed for Excel ----------------------------
    let (out, _, ok) = sandbox.run(&["export", "--csv", "--separator=;"]);
    assert!(ok);
    assert!(out.lines().next().unwrap().contains("album_artist;album"));
    let (_, err, ok) = sandbox.run(&["export", "--csv", "--separator=xyz"]);
    assert!(!ok);
    assert!(err.contains("one character"), "stderr: {err}");

    // --- A playlist of what is on screen -----------------------------------
    let (out, _, ok) = sandbox.run(&["album", "Kind of Blue", "--m3u"]);
    assert!(ok);
    assert!(out.starts_with("#EXTM3U"), "output: {out}");
    assert!(out.contains("#EXTINF:"), "durations and titles: {out}");
    assert!(out.contains(".flac"), "absolute paths: {out}");

    // --- Written to a file rather than printed ------------------------------
    let target = std::env::temp_dir().join("aede_e2e_export.m3u8");
    let _ = std::fs::remove_file(&target);
    let (out, _, ok) = sandbox.run(&[
        "album",
        "Kind of Blue",
        "--m3u",
        &format!("--output={}", target.display()),
    ]);
    assert!(ok, "output: {out}");
    let written = std::fs::read_to_string(&target).expect("the playlist file");
    assert!(written.starts_with("#EXTM3U"));
    let _ = std::fs::remove_file(&target);
}

#[test]
fn a_selection_can_be_exported_too() {
    let sandbox = Sandbox::new("selection");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // The same option on a selection gives that selection, not the library.
    let (whole, _, ok) = sandbox.run(&["export", "--csv", "--tracks"]);
    assert!(ok);
    let (one, _, ok) = sandbox.run(&["album", "Kind of Blue", "--csv"]);
    assert!(ok);
    assert!(
        one.lines().count() < whole.lines().count(),
        "one album is smaller than the catalog"
    );
    assert!(one.contains("Kind of Blue"));

    // An argument export cannot honour is refused, with the command that can.
    let (_, err, ok) = sandbox.run(&["export", "--csv", "Kind of Blue"]);
    assert!(!ok, "a stray argument must not be ignored");
    assert!(err.contains("takes no argument"), "stderr: {err}");
    assert!(err.contains("aede album"), "it points at the right command");
}

#[test]
fn every_listing_can_become_a_table() {
    // `--csv` used to be accepted everywhere and honoured on four commands.
    let sandbox = Sandbox::new("listing_csv");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    for (command, header) in [
        ("albums", "album_artist,album,year"),
        ("artists", "artist,sort_name,tracks"),
        ("genres", "genre,tracks,duration_ms"),
        ("labels", "label,albums,tracks"),
        ("years", "year,albums,tracks"),
    ] {
        let (out, _, ok) = sandbox.run(&[command, "--csv"]);
        assert!(ok, "{command} --csv must run");
        assert!(
            out.starts_with(header),
            "{command} header, got: {}",
            out.lines().next().unwrap_or("")
        );
    }

    // --output writes the file instead of printing it.
    let target = std::env::temp_dir().join("aede_e2e_listing.csv");
    let _ = std::fs::remove_file(&target);
    let (out, _, ok) = sandbox.run(&["albums", "--csv", &format!("--output={}", target.display())]);
    assert!(ok);
    assert!(out.contains("written to"), "it says where it went: {out}");
    let written = std::fs::read_to_string(&target).expect("the file");
    assert!(written.starts_with("album_artist,"), "content: {written}");
    let _ = std::fs::remove_file(&target);

    // A command that cannot honour the option refuses it.
    let (_, err, ok) = sandbox.run(&["stats", "--csv"]);
    assert!(!ok, "stats has no table to give");
    assert!(err.contains("cannot produce a table"), "stderr: {err}");
    let (_, err, ok) = sandbox.run(&["albums", "--m3u"]);
    assert!(!ok, "a list of albums is not a playlist");
    assert!(err.contains("cannot produce a playlist"), "stderr: {err}");

    // Naming two albums points at the command that lists several.
    let (_, err, ok) = sandbox.run(&["album", "Kind of Blue", "Sketches of Spain"]);
    assert!(!ok);
    assert!(err.contains("aede albums"), "stderr: {err}");
}

#[test]
fn resetting_asks_before_removing_the_catalog() {
    let sandbox = Sandbox::new("reset");
    let (_, _, ok) = sandbox.run(&["scan", library().to_str().unwrap()]);
    assert!(ok);

    // With no terminal to ask on, the command refuses rather than guessing.
    let (out, err, ok) = sandbox.run(&["reset"]);
    assert!(!ok, "it must not delete without an answer");
    assert!(err.contains("--yes"), "it says how to proceed: {err}");
    // And it says what is at stake before asking.
    assert!(out.contains("Watched folders"), "output: {out}");
    assert!(out.contains("Integrity verdicts"), "output: {out}");
    assert!(
        sandbox.dir.join("catalog.json").exists(),
        "the catalog is still there"
    );

    // Explicit consent removes it, and says how to get it back.
    let (out, _, ok) = sandbox.run(&["reset", "--yes"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("catalog removed"), "output: {out}");
    assert!(out.contains("aede scan"), "it prints the way back: {out}");
    assert!(
        !sandbox.dir.join("catalog.json").exists(),
        "the file is gone"
    );

    // The library is unreachable again, as after a fresh install.
    let (_, err, ok) = sandbox.run(&["stats"]);
    assert!(!ok);
    assert!(err.contains("aede scan"), "stderr: {err}");

    // Removing nothing is not an error.
    let (out, _, ok) = sandbox.run(&["reset", "--yes"]);
    assert!(ok);
    assert!(out.contains("no catalog to remove"), "output: {out}");
}

#[test]
fn an_album_query_does_not_pick_one_answer_in_silence() {
    let sandbox = Sandbox::new("album_match");
    let root = std::env::temp_dir().join("aede_e2e_album_match");
    let _ = std::fs::remove_dir_all(&root);
    for (folder, album) in [("one", "Danzig"), ("four", "Danzig 4")] {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(library().join("track.flac"), dir.join("1.flac")).unwrap();
        // The tag decides the album, not the folder name.
        let _ = album;
    }
    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // The fixture is one album ("Kind of Blue") in two folders, so asking for a
    // fragment must show both matches rather than one of them.
    let (out, _, ok) = sandbox.run(&["album", "kind of"]);
    assert!(ok, "output: {out}");
    assert!(
        out.contains("showing the titles containing it"),
        "it says the match is not exact: {out}"
    );

    // An exact title stops the search there.
    let (out, _, ok) = sandbox.run(&["album", "Kind of Blue"]);
    assert!(ok);
    assert!(
        !out.contains("showing the titles containing it"),
        "an exact title needs no widening: {out}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Writes a report describing `file` as another tool would have measured it.
///
/// The size and date are read from the file itself: an imported analysis is
/// bound to the bytes it was reached on, so a fixture with invented numbers
/// would only ever exercise the staleness path.
fn write_report(at: &std::path::Path, file: &std::path::Path, md5: &str, transcoding: &str) {
    let named_as = file.to_string_lossy().into_owned();
    write_report_naming(at, file, &named_as, md5, transcoding);
}

/// The same, but writing `named_as` as the path instead of where the file is.
///
/// This is not an academic case. Watched folders are stored canonical, so a
/// report written against a symbolic link — or against `/var` where macOS says
/// `/private/var` — names the very same file by another route, and the two
/// strings never match.
fn write_report_naming(
    at: &std::path::Path,
    file: &std::path::Path,
    named_as: &str,
    md5: &str,
    transcoding: &str,
) {
    let meta = std::fs::metadata(file).expect("the analysed file");
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let text = format!(
        r#"{{"format":"flaccompagnon-report","version":1,"report":{{
             "root":"{root}",
             "files":[{{
               "path":"{path}",
               "file_name":"{name}",
               "size_bytes":{size},
               "modified_unix":{mtime},
               "detections":{{"transcoding":"{transcoding}","upscaling":false,
                              "upsampling":false,"summary":"Clean",
                              "detail":"full-band content"}},
               "cutoff_hz":22050.0,
               "real_bit_depth":16,
               "dr_db":9.3,
               "clipping":{{"clipped_samples":0,"peak_dbfs":-0.13,"clipped":false}},
               "flac_md5":{{"state":"{md5}"}}
             }}]}}}}"#,
        root = file.parent().unwrap().display(),
        path = named_as,
        name = file.file_name().unwrap().to_str().unwrap(),
        size = meta.len(),
    );
    std::fs::write(at, text).expect("writing the report");
}

#[test]
fn another_tools_analysis_can_be_taken_in_and_given_back() {
    let sandbox = Sandbox::new("import");
    let root = std::env::temp_dir().join("aede_e2e_import_src");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let flac = root.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();

    let (_, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok);

    // --- Importing ----------------------------------------------------------
    let report = root.join("report.json");
    write_report(&report, &flac, "Match", "none");
    let (out, err, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok, "the import must succeed. stderr: {err}");
    assert!(out.contains("Files matched"), "output: {out}");
    assert!(out.contains("Analyses stored"), "output: {out}");

    // What was imported is attributed to whoever measured it, never merged into
    // Aède's own reading.
    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");
    assert!(out.contains("Dynamic range"), "output: {out}");
    assert!(!out.contains("stale"), "the file has not changed: {out}");

    // Importing the same report twice replaces; it does not accumulate.
    let (out, _, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Analyses stored"), "output: {out}");
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert_eq!(
        out.matches("Analysed by flaccompagnon").count(),
        1,
        "one analysis per file and per source:\n{out}"
    );

    // --- Two methods disagreeing --------------------------------------------
    let (_, _, ok) = sandbox.run(&["check"]);
    assert!(ok);
    write_report(&report, &flac, "Mismatch", "detected");
    let (_, _, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok);
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(
        out.contains("does not match its MD5"),
        "a failed MD5 is an error even when the checksums passed:\n{out}"
    );
    assert!(out.contains("re-encoded"), "and it says why:\n{out}");
    assert!(
        out.contains("made from a lossy source"),
        "the lossy ancestry is reported too:\n{out}"
    );

    // --- A report about other bytes -----------------------------------------
    // Once the file has been touched and read again, every imported verdict
    // about it is worthless: the catalog says so rather than answering with
    // confidence about bytes that are no longer there.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&flac)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let (_, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(out.contains("stale"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["doctor"]);
    assert!(ok);
    assert!(
        !out.contains("does not match its MD5"),
        "a stale verdict is not reported:\n{out}"
    );

    // And importing that same report again refuses it for the same reason,
    // instead of quietly restoring it.
    let (out, _, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Changed since the report"), "output: {out}");

    // --- Forgetting ---------------------------------------------------------
    let (out, _, ok) = sandbox.run(&["import", "--forget"]);
    assert!(ok, "output: {out}");
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert!(!out.contains("Analysed by"), "nothing is left: {out}");

    // --- Refusals -----------------------------------------------------------
    let foreign = root.join("foreign.json");
    std::fs::write(&foreign, r#"{"format":"something-else"}"#).unwrap();
    let (_, err, ok) = sandbox.run(&["import", foreign.to_str().unwrap()]);
    assert!(!ok, "another tool's JSON is refused");
    assert!(err.contains("not a FlacCompagnon report"), "stderr: {err}");

    let (_, err, ok) = sandbox.run(&["import", "/nowhere/at/all.json"]);
    assert!(!ok);
    assert!(err.contains("does not exist"), "stderr: {err}");

    let (_, err, ok) = sandbox.run(&["import"]);
    assert!(!ok, "an import with nothing to import is an error");
    assert!(err.contains("give a report"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_analysis_can_arrive_before_the_library_does() {
    // Analysing a folder and then building the library from it is the natural
    // order for someone who already owns the other tool. The import must
    // therefore not require the files to be known yet.
    let sandbox = Sandbox::new("import_first");
    let root = std::env::temp_dir().join("aede_e2e_import_first_src");
    let music = root.join("music");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&music).unwrap();
    let flac = music.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();

    // The report sits outside the library, so that the scan reaches it through
    // what was already imported rather than by walking over it. And it names
    // the file by a path the catalog will never hold: this is what a report
    // written against a symbolic link looks like, and on macOS what every
    // report under /var looks like once the folder is canonicalized to
    // /private/var. Only the name and the size can bridge the two.
    let report = root.join("report.json");
    let elsewhere = "/elsewhere/Danzig/01 So What.flac";
    write_report_naming(&report, &flac, elsewhere, "Match", "none");

    // Nothing has ever been scanned: the catalog does not exist yet.
    let (out, err, ok) = sandbox.run(&["import", report.to_str().unwrap()]);
    assert!(ok, "an import needs no catalog. stderr: {err}");
    assert!(out.contains("Waiting for a scan"), "output: {out}");
    assert!(
        out.contains("they attach themselves"),
        "and it says what happens next: {out}"
    );

    // The scan brings the file in, and the analysis attaches itself although
    // the path it names is not the path the file is at.
    let (out, _, ok) = sandbox.run(&["scan", music.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("Analyses now attached"), "output: {out}");
    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");
    assert!(!out.contains("stale"), "and it applies: {out}");

    // Attached for good: a second scan does not have to do it again.
    let (out, _, ok) = sandbox.run(&["scan"]);
    assert!(ok);
    assert!(!out.contains("Analyses now attached"), "output: {out}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_report_left_in_the_library_is_picked_up_by_the_scan() {
    // The report may equally well be sitting in the album folder. A scan walks
    // over it anyway, so it costs nothing to notice it.
    let sandbox = Sandbox::new("import_scan");
    let root = std::env::temp_dir().join("aede_e2e_import_scan_src");
    let deep = root.join("Danzig/1996 Blackacidevil");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&deep).unwrap();
    let flac = deep.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();
    write_report(&deep.join("analysis.json"), &flac, "Match", "none");
    // A JSON that is not a report is left alone, whatever it holds.
    std::fs::write(deep.join("other.json"), r#"{"hello": "world"}"#).unwrap();

    let (out, _, ok) = sandbox.run(&["scan", root.to_str().unwrap()]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Analyses imported"), "output: {out}");

    let (out, _, ok) = sandbox.run(&["track", "So What"]);
    assert!(ok);
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn reports_are_looked_for_in_every_folder_underneath() {
    // Reports are kept the way albums are: one folder per artist, one per
    // album. Only looking at the top level would find nothing.
    let sandbox = Sandbox::new("import_recursive");
    let root = std::env::temp_dir().join("aede_e2e_import_recursive_src");
    let music = root.join("music");
    let reports = root.join("reports/Danzig/1996 Blackacidevil");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&music).unwrap();
    std::fs::create_dir_all(&reports).unwrap();
    let flac = music.join("01 So What.flac");
    std::fs::copy(library().join("track.flac"), &flac).unwrap();
    write_report(&reports.join("album.json"), &flac, "Match", "none");

    let (_, _, ok) = sandbox.run(&["scan", music.to_str().unwrap()]);
    assert!(ok);

    let (out, _, ok) = sandbox.run(&["import", root.join("reports").to_str().unwrap()]);
    assert!(ok, "output: {out}");
    assert!(out.contains("Files matched"), "output: {out}");
    let (out, _, _) = sandbox.run(&["track", "So What"]);
    assert!(out.contains("Analysed by flaccompagnon"), "output: {out}");

    // A folder holding no report at all is an error, not a silent success.
    let (_, err, ok) = sandbox.run(&["import", music.to_str().unwrap()]);
    assert!(!ok);
    assert!(err.contains("no .json report"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&root);
}
