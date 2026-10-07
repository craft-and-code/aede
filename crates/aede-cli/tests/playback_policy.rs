#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use aede_core::{conclusions, user};

struct Library {
    root: PathBuf,
    source: PathBuf,
    data: PathBuf,
    ffplay: PathBuf,
}

impl Library {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aede_playback_policy_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("source.flac");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
            &source,
        )
        .unwrap();
        let ffplay = root.join("ffplay");
        std::fs::write(
            &ffplay,
            "#!/bin/sh\nprintf x >> \"$0.started\"\ncat >> \"$0.pcm\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&ffplay, std::fs::Permissions::from_mode(0o700)).unwrap();
        let data = root.join("data");
        Self {
            root,
            source,
            data,
            ffplay,
        }
    }

    fn run(&self, selection: &Path, options: &[&str], backend: &str) -> Output {
        let mut path = OsString::from(self.root.as_os_str());
        path.push(":");
        path.push(std::env::var_os("PATH").unwrap_or_default());
        Command::new(env!("CARGO_BIN_EXE_aede"))
            .arg("play")
            .arg(selection)
            .args(options)
            .arg("--data")
            .arg(&self.data)
            .env("PATH", path)
            .env("AEDE_AUDIO_BACKEND", backend)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn history(&self) -> Option<user::UserData> {
        user::load(&user::user_path(&self.data)).unwrap()
    }

    fn cache(&self) -> Option<conclusions::Conclusions> {
        conclusions::load(&conclusions::conclusions_path(&self.data)).unwrap()
    }

    fn assert_no_output_or_records(&self) {
        assert!(!self.ffplay.with_extension("started").exists());
        assert!(!self.ffplay.with_extension("pcm").exists());
        assert!(self.history().is_none());
        assert!(self.cache().is_none());
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn explicit_no_effects_and_strict_policies_refuse_effect_conflicts_before_output() {
    for policy in ["without-effects", "bit-perfect"] {
        for effect in ["--normalize=track", "--bass=3", "--treble=-3"] {
            let library = Library::new();
            let policy_option = format!("--playback={policy}");
            let result = library.run(
                &library.root.join("missing.flac"),
                &[&policy_option, "--output-device=alsa:hw:0,0", effect],
                "ffplay",
            );
            assert!(!result.status.success());
            assert!(String::from_utf8_lossy(&result.stderr).contains("select --playback=dsp"));
            library.assert_no_output_or_records();
        }
    }
}

#[test]
fn strict_playback_without_an_explicit_device_refuses_before_source_resolution() {
    let library = Library::new();
    let result = library.run(
        &library.root.join("missing.flac"),
        &["--playback=bit-perfect"],
        "native",
    );
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("requires an explicit --output-device")
    );
    library.assert_no_output_or_records();
}

#[test]
fn strict_playback_cannot_use_a_requested_ffplay_backend() {
    let library = Library::new();
    let result = library.run(
        &library.source,
        &["--playback=bit-perfect", "--output-device=alsa:hw:0,0"],
        "ffplay",
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot use ffplay"));
    library.assert_no_output_or_records();
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
#[test]
fn an_unavailable_alsa_host_is_refused_without_portable_output_fallback() {
    let library = Library::new();
    let result = library.run(
        &library.source,
        &[
            "--playback=bit-perfect",
            "--output-device=alsa:hw:CARD=0,DEV=0",
        ],
        "native",
    );
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("not supported on this platform")
            || error.contains("unavailable on this build"),
        "{error}"
    );
    library.assert_no_output_or_records();
}

#[test]
fn default_playback_keeps_effects_off_and_selected_dsp_learns_loudness() {
    for (options, policy, capture) in [
        (&[][..], "without-effects", false),
        (&["--normalize=off"][..], "without-effects", false),
        (&["--playback=dsp"][..], "dsp", true),
        (&["--bass=3"][..], "dsp", true),
    ] {
        let library = Library::new();
        let before = std::fs::read(&library.source).unwrap();
        let result = library.run(&library.source, options, "ffplay");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            stdout.contains(&format!("Playback policy: {policy}")),
            "{stdout}"
        );
        assert_eq!(stdout.contains("measuring source during playback"), capture);
        let history = library.history().unwrap();
        assert_eq!(history.plays.len(), 1);
        assert!(history.plays[0].completed);
        if capture {
            let cache = library
                .cache()
                .expect("DSP records verified source loudness");
            assert_eq!(cache.loudness_tracks.len(), 1);
            assert!(
                cache.loudness_tracks.contains_key(
                    &library
                        .source
                        .canonicalize()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                )
            );
        } else {
            assert!(
                library.cache().is_none(),
                "without-effects must not persist source loudness"
            );
        }
        assert_eq!(std::fs::read(&library.source).unwrap(), before);
    }
}

#[test]
fn bad_flac_md5_remains_terminal_with_effects_off_or_dsp_enabled() {
    for options in [
        &["--playback=without-effects"][..],
        &["--playback=dsp", "--bass=3"][..],
    ] {
        let library = Library::new();
        let invalid = library.root.join("wrong-md5.flac");
        let mut bytes = std::fs::read(&library.source).unwrap();
        assert_eq!(&bytes[..4], b"fLaC");
        assert_eq!(bytes[4] & 0x7f, 0);
        assert_eq!(&bytes[5..8], &[0, 0, 34]);
        assert!(bytes[26..42].iter().any(|&byte| byte != 0));
        bytes[26] ^= 1;
        std::fs::write(&invalid, bytes).unwrap();
        let unvisited = library.root.join("unvisited.flac");
        std::fs::copy(&library.source, &unvisited).unwrap();
        let playlist = library.root.join("selection.m3u");
        std::fs::write(
            &playlist,
            format!(
                "{}\n{}\n{}\n",
                library.source.display(),
                invalid.display(),
                unvisited.display()
            ),
        )
        .unwrap();
        let result = library.run(&playlist, options, "ffplay");
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("MD5"));
        let history = library.history().unwrap();
        assert_eq!(history.plays.len(), 2);
        assert_eq!(
            Path::new(&history.plays[0].track.key),
            library.source.canonicalize().unwrap()
        );
        assert!(history.plays[0].completed);
        assert_eq!(history.plays[0].ms_played, 1000);
        assert_eq!(
            Path::new(&history.plays[1].track.key),
            invalid.canonicalize().unwrap()
        );
        assert!(!history.plays[1].completed);
        assert!(history.plays[1].ms_played > 0 && history.plays[1].ms_played <= 1000);
        assert!(
            history
                .plays
                .iter()
                .all(|listen| Path::new(&listen.track.key) != unvisited)
        );
        if options[0] == "--playback=without-effects" {
            assert!(library.cache().is_none());
        } else {
            let cache = library.cache().unwrap();
            assert_eq!(cache.loudness_tracks.len(), 1);
            assert!(
                cache.loudness_tracks.contains_key(
                    &library
                        .source
                        .canonicalize()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                )
            );
            assert!(cache.loudness_programmes.is_empty());
        }
    }
}
