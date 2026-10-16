//! Integration tests for the FLAC frame walk.
//!
//! The original five reference files were produced with ffmpeg, each built to carry
//! a single property: real 24-bit content, 16-bit content packaged as 24, two
//! identical channels, two different ones, and digital silence.
//! Additional tiny Xiph encodings cover correlated/constant precision edges;
//! their reproducible provenance is in `fixtures/audit-precision.md`.

use std::path::PathBuf;

use aede_core::audit::{self, Limits, StereoContent};

fn walk(name: &str) -> audit::FlacAudit {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    audit::flac::audit(&path, Limits::thorough()).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn real_24_bit_content_is_left_alone() {
    let a = walk("audit-real24.flac");
    assert_eq!(a.declared_bit_depth, 24);
    assert_eq!(a.effective_bit_depth, 24, "nothing to take away");
    assert!(!a.is_padded());
}

#[test]
fn padding_is_seen_through() {
    // 16-bit music re-encoded as 24-bit: the encoder records eight wasted bits
    // in every subframe instead of storing zeros.
    let a = walk("audit-padded24.flac");
    assert_eq!(a.declared_bit_depth, 24);
    assert_eq!(a.effective_bit_depth, 16, "eight wasted bits");
    assert!(a.is_padded());
}

#[test]
fn two_identical_channels_are_recognised() {
    let a = walk("audit-dualmono.flac");
    assert_eq!(a.channels, 2);
    assert_eq!(a.stereo, StereoContent::Duplicated);
    assert!(!a.digital_silence, "the file does carry sound");
}

#[test]
fn real_stereo_is_not_accused() {
    let a = walk("audit-stereo.flac");
    assert_eq!(a.stereo, StereoContent::Independent);
    assert_eq!(a.effective_bit_depth, a.declared_bit_depth);
}

#[test]
fn digital_silence_is_reported() {
    let a = walk("audit-silence.flac");
    assert!(a.digital_silence, "every subframe is a constant zero");
}

#[test]
fn a_complete_walk_is_not_reported_as_truncated() {
    let a = walk("audit-stereo.flac");
    assert!(!a.truncated, "the whole file was read");
    assert!(a.frames_examined > 0);
}

#[test]
fn a_limit_marks_the_result_as_partial() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/audit-stereo.flac");
    let limits = Limits {
        max_bytes: 64 * 1024,
        max_frames: 1,
    };
    let a = audit::flac::audit(&path, limits).expect("walk");
    assert_eq!(a.frames_examined, 1);
    assert!(a.truncated, "the verdict covers one frame only");
}

#[test]
fn a_file_that_is_not_flac_is_refused() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.mp3");
    assert!(audit::flac::audit(&path, Limits::quick()).is_err());
}

#[test]
fn significant_low_bits_survive_stereo_and_constant_subframes() {
    // Real encodings: mid/side rounding, one nonzero constant channel, and
    // whole nonzero constant frames on either side of a padded frame.
    for (name, frames) in [
        ("audit-mid-side-lsb24.flac", 1),
        ("audit-constant-lsb24.flac", 1),
        ("audit-dc-then-padded24.flac", 2),
        ("audit-padded-then-dc24.flac", 2),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let original = std::fs::read(&path).expect("fixture bytes");
        let a = walk(name);
        assert_eq!(a.declared_bit_depth, 24, "{name}");
        assert_eq!(a.effective_bit_depth, 24, "{name} has nonzero low bits");
        assert!(!a.is_padded(), "{name}");
        assert_eq!(a.frames_examined, frames, "{name}");
        assert!(!a.digital_silence, "{name}");
        assert!(!a.truncated, "{name}");
        let integrity = audit::integrity::check(&path).expect("fixture checksum walk");
        assert_eq!(
            integrity.verdict,
            audit::integrity::Verdict::Intact,
            "{name}"
        );
        assert_eq!(std::fs::read(&path).expect("fixture after audit"), original);
    }
}
