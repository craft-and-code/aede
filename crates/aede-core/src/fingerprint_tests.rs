//! Tests for [`super`], split out of `fingerprint.rs`.
//!
//! The parsing is tested on canned output, and the whole thing is tested
//! **against a real ffmpeg** when the one on this machine can do it — which
//! is the only way to know that the flags are the right flags. A fingerprint
//! computed with the wrong algorithm is not wrong-looking: it is a valid
//! fingerprint that matches nothing.

use super::*;

#[path = "fingerprint_test_support.rs"]
mod test_support;
use test_support::AudioFolder;

#[test]
fn malformed_fractional_durations_cannot_identify_a_recording() {
    for duration in [
        "183.NaN",
        "183.4.5",
        "183.",
        "183.-4",
        "+183",
        "NaN",
        "inf",
        "4294967296",
    ] {
        let output = format!("DURATION={duration}\nFINGERPRINT=AQAA\n");
        assert!(read_fpcalc(&output).is_err(), "duration: {duration}");
    }
}

#[test]
fn fingerprinting_requires_a_local_regular_file_before_running_a_helper() {
    let folder = AudioFolder::new();
    for by in [By::Ffmpeg, By::Fpcalc] {
        for path in [
            std::path::Path::new("file:///aede-missing.flac"),
            folder.path(),
        ] {
            let error = of(by, path, 30).expect_err("not a local regular file");
            assert!(error.contains("local regular file"), "{by:?}: {error}");
        }
        #[cfg(unix)]
        {
            let error =
                of(by, std::path::Path::new("/dev/null"), 30).expect_err("not a regular file");
            assert!(error.contains("local regular file"), "{by:?}: {error}");
        }
    }
}

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
    let folder = AudioFolder::new();
    let dir = folder.path();
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

    // **The premise of having two paths at all**: on a machine with both, they
    // must answer the same string, or a library fingerprinted on one machine
    // would match nothing fingerprinted on another. The flags are what make it
    // true — ffmpeg is given `-algorithm 1` and fpcalc is given nothing, and
    // `fpcalc -algorithm 1` would be a *different* algorithm, because the two
    // programs number the list differently. Nothing about that is inferable
    // from either manual; it is measured, here, whenever the machine can.
    match has_chromaprint() && has_fpcalc() {
        true => {
            let one = of(By::Ffmpeg, &path, 30).expect("ffmpeg has chromaprint");
            let two = of(By::Fpcalc, &path, 30).expect("fpcalc is here");
            assert_eq!(
                one.data, two.data,
                "ffmpeg and fpcalc must agree, or the fallback is not a fallback"
            );
        }
        false => eprintln!("skipped: this machine does not have both programs"),
    }
}

#[test]
fn fpcalc_measures_the_whole_audio_including_changes_after_two_minutes() {
    if !has_fpcalc() {
        eprintln!("skipped: fpcalc is not installed");
        return;
    }
    let folder = AudioFolder::new();
    let mut observed = Vec::new();
    let ffmpeg_available = has_chromaprint();
    for (name, seconds, changed_tail) in [
        ("short.wav", 30, false),
        ("boundary.wav", 120, false),
        ("past-boundary.wav", 121, false),
        ("complete.wav", 150, false),
        ("changed-tail.wav", 150, true),
    ] {
        let path = folder.tone(name, seconds, changed_tail);
        let original = std::fs::read(&path).expect("the generated source");
        // An explicit complete-input reference, independent of Aède's command
        // construction. The short case also retains the ordinary helper's
        // established output; the distinct tails detect a hidden preview.
        let output = std::process::Command::new("fpcalc")
            .args(["-length", "0"])
            .arg(&path)
            .output()
            .expect("run the reference helper");
        assert!(output.status.success(), "{name}: {:?}", output.stderr);
        let expected = read_fpcalc(&String::from_utf8(output.stdout).expect("text output"))
            .expect("a complete reference fingerprint");
        assert_eq!(
            expected.seconds, seconds,
            "the full source duration: {name}"
        );
        let found = of(By::Fpcalc, &path, seconds).expect("the production fallback");
        assert_eq!(
            std::fs::read(&path).expect("the unchanged source"),
            original,
            "fingerprinting never modifies {name}"
        );
        if ffmpeg_available {
            let ffmpeg = of(By::Ffmpeg, &path, seconds).expect("the production FFmpeg path");
            assert_eq!(ffmpeg, expected, "the complete-input helpers agree: {name}");
        }
        observed.push((name, expected, found));
    }
    assert_ne!(
        observed[3].1.data, observed[4].1.data,
        "the reference must hear the changed final thirty seconds"
    );
    for (name, expected, found) in observed {
        assert_eq!(found, expected, "the whole source is fingerprinted: {name}");
    }
}
