//! Tests for [`super`], split out of `fingerprint.rs`.
//!
//! The parsing is tested on canned output, and the whole thing is tested
//! **against a real ffmpeg** when the one on this machine can do it — which
//! is the only way to know that the flags are the right flags. A fingerprint
//! computed with the wrong algorithm is not wrong-looking: it is a valid
//! fingerprint that matches nothing.

use super::*;

#[test]
fn ffmpeg_output_is_the_fingerprint_and_the_caller_supplies_the_length() {
    let found = read_ffmpeg("  AQAA3UmUaEkSZSoAAAAA\n", 183).expect("a fingerprint");
    assert_eq!(found.data, "AQAA3UmUaEkSZSoAAAAA");
    assert_eq!(found.seconds, 183);
}

#[test]
fn fpcalc_states_its_own_duration_and_it_wins() {
    // It measured what it decoded; the catalog holds what a header claims,
    // and a file where the two disagree is exactly what this feature is for.
    let found = read_fpcalc("DURATION=183\nFINGERPRINT=AQAAcxUmUaEk\n").expect("a fingerprint");
    assert_eq!(found.data, "AQAAcxUmUaEk");
    assert_eq!(found.seconds, 183);

    // Order is not promised, and a fractional duration must not read as none.
    let other = read_fpcalc("FINGERPRINT=AQAA\nDURATION=183.4\n").expect("a fingerprint");
    assert_eq!(other.seconds, 183);
}

#[test]
fn an_empty_answer_is_a_refusal_and_never_an_empty_fingerprint() {
    // The guard that matters: a blank fingerprint sent to AcoustID asks it to
    // match silence against its whole index, and whatever comes back would be
    // filed against this file as though somebody had identified it.
    assert!(read_ffmpeg("", 183).is_err());
    assert!(read_ffmpeg("   \n", 183).is_err());
    assert!(read_fpcalc("DURATION=183\n").is_err());
    assert!(read_fpcalc("").is_err());

    // And a fingerprint with no length is refused too, because the lookup
    // needs both and a zero would be sent as a real number.
    let none = read_ffmpeg("AQAA", 0).expect_err("no length");
    assert!(none.contains("duration"), "{none}");
    assert!(read_fpcalc("FINGERPRINT=AQAA\nDURATION=0\n").is_err());
}

#[test]
fn the_message_for_a_machine_with_neither_program_names_both_ways_out() {
    let said = missing();
    assert!(said.contains("chromaprint") && said.contains("ffmpeg"));
    assert!(said.contains("brew install") && said.contains("apt install"));
    // The right answer differs by platform, and the message says so rather
    // than offering one list and letting the reader find out. Homebrew's
    // ffmpeg has no chromaprint — this module once claimed it did, and a
    // reader's `configuration:` line disproved it.
    assert!(said.contains("brew install chromaprint"), "{said}");
    assert!(
        said.contains("Homebrew's ffmpeg is built without chromaprint"),
        "the reason the ffmpeg they already have will not do: {said}"
    );
    assert!(
        said.contains("Everything else in Aède works without either"),
        "a missing optional program must not read as a broken installation"
    );
}

#[test]
fn a_real_file_fingerprints_the_same_way_twice() {
    // The only test here that proves the *flags* are right, and it runs only
    // where the machine can actually do it — a checkout with an ffmpeg built
    // without chromaprint reports a skip rather than a pass, because passing
    // would say something this test did not verify.
    let Some(by) = find() else {
        eprintln!("skipped: no ffmpeg with chromaprint and no fpcalc on this machine");
        return;
    };

    // Thirty seconds of a tone: long enough to fingerprint, and generated
    // rather than committed so the repository carries no audio for one test.
    let Some(ffmpeg) = crate::ffmpeg::find() else {
        eprintln!("skipped: no ffmpeg to make the fixture with");
        return;
    };
    let dir = std::env::temp_dir().join("aede_fingerprint");
    std::fs::create_dir_all(&dir).expect("a folder");
    let path = dir.join("tone.flac");
    let made = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=30"])
        .args(["-ac", "2", "-ar", "44100"])
        .arg(&path)
        .status();
    if !made.is_ok_and(|s| s.success()) {
        eprintln!("skipped: this ffmpeg could not make the fixture");
        return;
    }

    let first = of(by, &path, 30).expect("a fingerprint");
    assert!(
        first.data.starts_with("AQ"),
        "a base64 chromaprint fingerprint begins with its version byte: {}",
        first.data
    );
    assert_eq!(first.seconds, 30);

    // Deterministic, which is the whole premise: the same audio must answer
    // the same thing, or nothing downstream can be cached or compared.
    let again = of(by, &path, 30).expect("a fingerprint");
    assert_eq!(first, again);

    // And a file that is not audio is an error rather than an empty answer.
    let nonsense = dir.join("nonsense.flac");
    std::fs::write(&nonsense, b"not a FLAC file at all").expect("written");
    assert!(of(by, &nonsense, 30).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}
