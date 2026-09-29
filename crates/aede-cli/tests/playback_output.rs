#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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
    let mono = fixtures.join("hires.flac");
    let playlist = root.join("play.m3u");
    std::fs::write(
        &playlist,
        format!(
            "{}\n{}\n{}\n",
            stereo.display(),
            stereo.display(),
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
    assert!(
        !std::fs::read(ffplay.with_extension("data"))
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}
