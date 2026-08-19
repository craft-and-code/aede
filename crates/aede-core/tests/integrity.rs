//! Integration tests for the integrity check, against deliberately damaged
//! copies of real files.
//!
//! Damage is inflicted here rather than committed as a fixture: a corrupt file
//! in the repository would look like a mistake, and the exact byte flipped is
//! part of what the test says.

use std::path::{Path, PathBuf};

use aede_core::audit::integrity::{self, Verdict};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Copies a fixture, flipping one bit `from_end` bytes before its end.
fn damaged_copy(name: &str, from_end: usize, suffix: &str) -> PathBuf {
    let mut bytes = std::fs::read(fixture(name)).expect("reading the fixture");
    let index = bytes.len() - from_end;
    bytes[index] ^= 0x01;
    let path = std::env::temp_dir().join(format!("aede_damaged_{suffix}_{name}"));
    std::fs::write(&path, &bytes).expect("writing the copy");
    path
}

fn verdict(path: &Path) -> Verdict {
    integrity::check(path)
        .unwrap_or_else(|e| panic!("checking {}: {e}", path.display()))
        .verdict
}

#[test]
fn an_intact_flac_passes() {
    let report = integrity::check(&fixture("track.flac")).expect("check");
    assert_eq!(report.verdict, Verdict::Intact);
    assert_eq!(report.method, integrity::FLAC_METHOD);
    assert!(report.units > 0, "frames verified: {}", report.units);
}

#[test]
fn an_intact_ogg_passes() {
    for name in ["track.ogg", "track.opus", "track.spx"] {
        let report = integrity::check(&fixture(name)).expect("check");
        assert_eq!(report.verdict, Verdict::Intact, "{name}");
        assert_eq!(report.method, integrity::OGG_METHOD, "{name}");
    }
}

#[test]
fn a_flipped_bit_in_a_flac_is_caught() {
    let path = damaged_copy("track.flac", 200, "bit");
    assert!(
        matches!(verdict(&path), Verdict::Damaged { .. }),
        "one flipped bit must not go unnoticed"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_flipped_bit_in_an_ogg_page_is_caught() {
    let path = damaged_copy("track.ogg", 200, "bit");
    match verdict(&path) {
        Verdict::Damaged { detail } => assert!(detail.contains("page"), "detail: {detail}"),
        other => panic!("expected a damaged verdict, got {other:?}"),
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_truncated_flac_is_caught() {
    // Every surviving frame is perfectly valid: what gives the file away is
    // that the last one does not end where the file does.
    let bytes = std::fs::read(fixture("track.flac")).unwrap();
    let path = std::env::temp_dir().join("aede_truncated_check.flac");
    std::fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
    assert!(matches!(verdict(&path), Verdict::Damaged { .. }));
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_container_without_a_checksum_says_so() {
    // Not an error, and not "intact" either: there is simply nothing to check,
    // and running the test again will never change that.
    for name in ["track.mp3", "track.m4a", "track.wav", "track.aiff"] {
        let report = integrity::check(&fixture(name)).expect("check");
        assert_eq!(report.verdict, Verdict::NothingToCheck, "{name}");
    }
}

#[test]
fn an_unreadable_file_is_an_error_not_a_verdict() {
    let path = std::env::temp_dir().join("aede_integrity_missing.flac");
    let _ = std::fs::remove_file(&path);
    assert!(integrity::check(&path).is_err());
}
