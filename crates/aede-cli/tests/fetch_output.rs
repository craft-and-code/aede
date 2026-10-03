//! Public M1 previews and refusals never mutate the library or reveal keys.

#[cfg(any(feature = "fetch", unix))]
mod fetch_output_support;
#[cfg(any(feature = "fetch", unix))]
use fetch_output_support::Library;
#[cfg(feature = "fetch")]
#[path = "fetch_output_support/filesystem.rs"]
mod filesystem;
#[cfg(feature = "fetch")]
use filesystem::snapshot;

#[cfg(feature = "fetch")]
#[test]
fn a_combined_fetch_preview_is_offline_private_and_writes_no_outputs() {
    let library = Library::new("Kind of Blue", false);
    let before = snapshot(&library.directory);
    let output = library.run(&[
        "fetch",
        "--dry-run",
        "--full",
        "--labels",
        "--summaries",
        "--discography",
        "--covers",
        "--lyrics",
        "--identify",
        "--credits",
        "--portraits",
        "--fanart",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(shown.contains("nothing was asked: --dry-run"));
    for section in [
        "Summaries",
        "Discography",
        "Cover art",
        "Lyrics",
        "Identify",
        "Recording and work credits",
        "Edition credits",
        "Portraits",
        "Fanart.tv artwork",
    ] {
        assert!(shown.contains(section), "missing requested pass: {section}");
    }
    let errors = String::from_utf8_lossy(&output.stderr);
    for secret in [
        "private-test-acoustid",
        "private-test-fanart",
        "fingerprint-sentinel",
    ] {
        assert!(
            !shown.contains(secret) && !errors.contains(secret),
            "preview disclosed a private request value"
        );
    }
    assert!(
        snapshot(&library.directory) == before,
        "preview mutated files"
    );
}

#[cfg(feature = "fetch")]
#[test]
fn contradictory_label_logo_options_are_refused_before_work() {
    let library = Library::new("Kind of Blue", false);
    let before = snapshot(&library.directory);
    let output = library.run(&[
        "fetch",
        "--fanart",
        "--logos",
        "--no-label-logo",
        "--dry-run",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--logos and --no-label-logo"));
    assert!(
        snapshot(&library.directory) == before,
        "refused preview mutated files"
    );
}

#[cfg(any(feature = "fetch", unix))]
#[test]
fn missing_recognises_a_complete_empty_discography() {
    let library = Library::new("Kind of Blue", false);
    let path = aede_core::sources::sources_path(&library.data);
    let mut held = aede_core::sources::load(&path).unwrap().unwrap();
    let aede_core::sources::Facts::Artist(facts) = &mut held.records[0].facts else {
        panic!("artist facts");
    };
    facts.discography_fetched_at = Some(17);
    aede_core::sources::save(&held, &path).unwrap();
    let output = library.run(&["missing"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(
        !shown.contains("no discography has been fetched"),
        "a completed empty answer was fetched: {shown}"
    );
    assert!(
        shown.contains("every studio album MusicBrainz credits to your artists is here"),
        "the missing report must use the empty answer: {shown}"
    );
    #[cfg(feature = "fetch")]
    {
        let preview = library.run(&["fetch", "--discography", "--dry-run"]);
        assert!(preview.status.success());
        let shown = String::from_utf8_lossy(&preview.stdout);
        assert!(
            shown.contains("stored discographies are already complete"),
            "a cached empty answer must not suggest running the identity fetch first: {shown}"
        );
    }
    held.records[0].source = "discogs".into();
    aede_core::sources::save(&held, &path).unwrap();
    let output = library.run(&["missing"]);
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("no discography has been fetched"),
        "another source's cache must not claim that a MusicBrainz browse succeeded"
    );
}

#[cfg(all(unix, feature = "fetch"))]
#[test]
fn fetch_previews_display_album_controls_literally() {
    let library = Library::new("Album\x1b[31m\rInjected", false);
    let output = library.run(&["fetch", "--covers", "--dry-run"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shown = String::from_utf8_lossy(&output.stdout);
    assert!(
        !shown.contains('\x1b'),
        "the album injected a terminal control"
    );
    assert!(
        !shown.contains('\r'),
        "the album injected a carriage return"
    );
    assert!(
        shown.contains("Album\\u{1b}[31m\\rInjected"),
        "the album remains visible: {shown}"
    );
}

#[cfg(unix)]
#[test]
fn extraction_errors_display_folder_controls_literally() {
    let library = Library::new("Album\x1b[2J\r", true);
    let original = std::fs::read(&library.audio).unwrap();
    let output = library.run(&["extract"]);
    let shown = String::from_utf8_lossy(&output.stderr);
    assert!(
        !shown.contains('\x1b'),
        "the folder injected a terminal control"
    );
    assert!(
        !shown.contains('\r'),
        "the folder injected a carriage return"
    );
    assert!(
        shown.contains("Album\\u{1b}[2J\\r"),
        "the path remains visible: {shown}"
    );
    assert_eq!(std::fs::read(&library.audio).unwrap(), original);
    assert_eq!(std::fs::read_dir(&library.music).unwrap().count(), 1);
}
