#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use aede_core::playback::stream::PcmTrack;
use aede_core::user;

fn pcm(path: &std::path::Path) -> Vec<u8> {
    let mut track = PcmTrack::open(path).unwrap();
    let mut bytes = Vec::new();
    while let Some(block) = track
        .read_block(|_| Ok::<(), std::convert::Infallible>(()))
        .unwrap()
    {
        bytes.extend_from_slice(block.f32le);
    }
    bytes
}

#[test]
fn cli_reuses_the_output_for_matching_tracks_and_reopens_on_format_change() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aede_cli_output_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let ffplay = root.join("ffplay");
    std::fs::write(
        &ffplay,
        "#!/bin/sh\nprintf x >> \"$0.count\"\ncat >> \"$0.data\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&ffplay, std::fs::Permissions::from_mode(0o700)).unwrap();

    let fixtures =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures");
    let stereo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/playback_fixtures/flac/playback-stereo.flac");
    let mp3 = fixtures.join("gapless-stereo.mp3");
    let mono = fixtures.join("hires.flac");
    let playlist = root.join("play.m3u");
    std::fs::write(
        &playlist,
        format!(
            "{}\n{}\n{}\n",
            stereo.display(),
            mp3.display(),
            mono.display()
        ),
    )
    .unwrap();

    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path = std::ffi::OsString::from(root.as_os_str());
    path.push(":");
    path.push(old_path);
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args([
            "play",
            &playlist.to_string_lossy(),
            "--normalize",
            "off",
            "--data",
            &root.join("data").to_string_lossy(),
        ])
        .env("PATH", path)
        .env("AEDE_AUDIO_BACKEND", "ffplay")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read(ffplay.with_extension("count")).unwrap(),
        b"xx"
    );
    let mut expected = pcm(&stereo);
    expected.extend_from_slice(&pcm(&mp3));
    expected.extend_from_slice(&pcm(&mono));
    assert_eq!(
        std::fs::read(ffplay.with_extension("data")).unwrap(),
        expected
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cli_rejects_an_unknown_audio_backend() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/playback_fixtures/flac/playback-stereo.flac");
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args(["play", &fixture.to_string_lossy()])
        .env("AEDE_AUDIO_BACKEND", "unknown")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must be native or ffplay"));
}

#[test]
fn missing_loudness_on_later_tracks_does_not_delay_or_prevent_the_first_output() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aede_lazy_loudness_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let music = root.join("music");
    std::fs::create_dir_all(&music).expect("music directory");
    let first = music.join("01.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-core/tests/playback_fixtures/flac/playback-stereo.flac"),
        &first,
    )
    .expect("first fixture");
    std::fs::write(music.join("02.flac"), b"not decodable audio").expect("bad later file");
    let ffplay = root.join("ffplay");
    std::fs::write(&ffplay, "#!/bin/sh\ncat > \"$0.data\"\n").expect("output stub");
    std::fs::set_permissions(&ffplay, std::fs::Permissions::from_mode(0o700)).expect("executable");
    let mut path = std::ffi::OsString::from(root.as_os_str());
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args([
            "play",
            &music.to_string_lossy(),
            "--data",
            &root.join("data").to_string_lossy(),
        ])
        .env("PATH", path)
        .env("AEDE_AUDIO_BACKEND", "ffplay")
        .output()
        .expect("playback");
    assert!(!result.status.success(), "bad later file is reported");
    assert_eq!(
        std::fs::read(ffplay.with_extension("data")).expect("first PCM output"),
        pcm(&first)
    );
    let history = user::load(&user::user_path(&root.join("data")))
        .expect("history")
        .expect("saved");
    assert_eq!(history.plays.len(), 1);
    assert!(history.plays[0].completed);
    assert!(String::from_utf8_lossy(&result.stdout).contains("measuring source during playback"));
    std::fs::remove_dir_all(root).expect("cleanup");
}

fn played_pcm(fixture: &str, normalization: Option<&str>) -> (Vec<f32>, user::UserData, String) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aede_normalization_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let ffplay = root.join("ffplay");
    std::fs::write(&ffplay, "#!/bin/sh\ncat > \"$0.data\"\n").unwrap();
    std::fs::set_permissions(&ffplay, std::fs::Permissions::from_mode(0o700)).unwrap();

    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let data_dir = root.join("data");
    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path = std::ffi::OsString::from(root.as_os_str());
    path.push(":");
    path.push(old_path);
    let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
    command
        .arg("play")
        .arg(&fixture)
        .arg("--data")
        .arg(&data_dir)
        .env("PATH", path)
        .env("AEDE_AUDIO_BACKEND", "ffplay");
    if let Some(mode) = normalization {
        command.arg("--normalize").arg(mode);
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(ffplay.with_extension("data")).unwrap();
    let samples = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|bytes| f32::from_le_bytes(*bytes))
        .collect();
    let history = user::load(&user::user_path(&data_dir)).unwrap().unwrap();
    let stderr = String::from_utf8_lossy(&result.stderr).into_owned();
    std::fs::remove_dir_all(root).unwrap();
    (samples, history, stderr)
}

fn assert_scaled(reference: &[f32], actual: &[f32], gain_db: f32) {
    assert_eq!(actual.len(), reference.len());
    let gain = 10.0_f32.powf(gain_db / 20.0);
    assert!(reference.iter().any(|sample| sample.abs() > 0.001));
    for (before, after) in reference.iter().zip(actual) {
        assert!((after - before * gain).abs() < 0.000_02);
    }
}

#[test]
fn metadata_normalization_applies_track_and_album_gain_without_changing_history() {
    let (track_by_default, default_history, _) = played_pcm("normalization.flac", None);
    let (off, off_history, _) = played_pcm("normalization.flac", Some("off"));
    let (track, track_history, _) = played_pcm("normalization.flac", Some("track"));
    let (album, album_history, _) = played_pcm("normalization.flac", Some("album"));
    assert_eq!(track_by_default, track);
    assert_scaled(&off, &track, -6.0206);
    assert_scaled(&off, &album, -12.0412);
    for history in [default_history, off_history, track_history, album_history] {
        assert_eq!(history.plays.len(), 1);
        assert!(history.plays[0].completed);
        assert_eq!(history.counts.len(), 1);
        assert_eq!(history.counts[0].count, 1);
    }
}

#[test]
fn opus_r128_gain_is_applied_after_its_header_output_gain() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        return;
    }
    let (off, _, _) = played_pcm("normalization-safe.opus", Some("off"));
    let (zero_header, _, _) = played_pcm("normalization-zero.opus", Some("off"));
    let (track, _, _) = played_pcm("normalization-safe.opus", Some("track"));
    let (album, _, _) = played_pcm("normalization-safe.opus", Some("album"));
    // The fixture has +1 dB Opus header gain, already applied by ffmpeg.
    // R128 values -2048 and -2560 are relative to that header gain.
    assert_scaled(&zero_header, &off, 1.0);
    assert_scaled(&off, &track, -3.0);
    assert_scaled(&off, &album, -5.0);
}

#[test]
fn positive_opus_r128_gain_without_a_peak_is_capped() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        return;
    }
    let (off, _, _) = played_pcm("normalization.opus", Some("off"));
    let (track, _, stderr) = played_pcm("normalization.opus", Some("track"));
    assert_scaled(&off, &track, 0.0);
    assert!(!stderr.contains("hard-clamped"), "{stderr}");
}

#[test]
fn missing_peak_keeps_normalization_from_boosting_above_full_scale() {
    let (original, _, _) = played_pcm("normalization-hot.flac", Some("off"));
    let (protected, _, stderr) = played_pcm("normalization-hot.flac", Some("track"));
    assert_scaled(&original, &protected, 0.0);
    assert!(protected.iter().all(|sample| sample.abs() <= 1.0));
    assert!(!stderr.contains("hard-clamped"), "{stderr}");
}

#[test]
fn replaygain_peak_allows_only_a_safe_positive_gain() {
    let (original, _, _) = played_pcm("normalization-peak.flac", Some("off"));
    let (protected, _, stderr) = played_pcm("normalization-peak.flac", Some("track"));
    assert_scaled(&original, &protected, 20.0 * (1.0_f32 / 0.05).log10());
    assert!(protected.iter().all(|sample| sample.abs() <= 1.0));
    assert!(!stderr.contains("hard-clamped"), "{stderr}");
}

#[test]
fn inaccurate_peak_is_hard_clamped_and_reported_before_output() {
    let (protected, _, stderr) = played_pcm("normalization-incorrect-peak.flac", Some("track"));
    assert!(protected.iter().all(|sample| sample.abs() <= 1.0));
    assert!(protected.iter().any(|sample| sample.abs() == 1.0));
    assert!(stderr.contains("hard-clamped"), "{stderr}");
}

#[test]
fn invalid_normalization_mode_is_an_error() {
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/normalization.flac");
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .arg("play")
        .arg(fixture)
        .args(["--normalize", "loud"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("off, track or album"));
}

#[test]
fn submillisecond_audio_does_not_create_a_listening_event() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aede_short_play_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let ffplay = root.join("ffplay");
    std::fs::write(&ffplay, "#!/bin/sh\ncat >/dev/null\n").unwrap();
    std::fs::set_permissions(&ffplay, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut path = std::ffi::OsString::from(root.as_os_str());
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    let data_dir = root.join("data");
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/submillisecond.flac");
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .arg("play")
        .arg(fixture)
        .arg("--data")
        .arg(&data_dir)
        .env("PATH", path)
        .env("AEDE_AUDIO_BACKEND", "ffplay")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(user::load(&user::user_path(&data_dir)).unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
}
