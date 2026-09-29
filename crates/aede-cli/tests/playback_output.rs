#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use aede_core::playback::stream::PcmTrack;

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
    let stereo = fixtures.join("audit-stereo.flac");
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
        .join("../aede-core/tests/fixtures/audit-stereo.flac");
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args(["play", &fixture.to_string_lossy()])
        .env("AEDE_AUDIO_BACKEND", "unknown")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must be native or ffplay"));
}
