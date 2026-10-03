//! Copy safety regressions run against the public command and real fixtures.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    directory: PathBuf,
    music: PathBuf,
    output: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "aede_copy_safety_{name}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let music = directory.join("music");
        let output = directory.join("output");
        let data = directory.join("data");
        for path in [&music, &output, &data] {
            std::fs::create_dir(path).unwrap();
        }
        Self {
            directory,
            music,
            output,
            data,
        }
    }

    fn audio(&self, relative: &str) -> PathBuf {
        let path = self.music.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac");
        std::fs::copy(source, &path).unwrap();
        path
    }

    fn run(&self, words: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(words)
            .env("AEDE_HOME", &self.data)
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    fn scan(&self) {
        let result = self.run(&["scan", self.music.to_str().unwrap()]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn copy(&self, options: &[&str]) -> std::process::Output {
        let mut words = vec!["copy", self.output.to_str().unwrap()];
        words.extend_from_slice(options);
        self.run(&words)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn dry_run_refuses_invalid_worker_counts_without_writing() {
    let fixture = Fixture::new("invalid_threads");
    fixture.audio("Album/01.flac");
    fixture.scan();
    let result = fixture.copy(&["--dry-run", "--threads", "invalid", "--extras", "none"]);
    assert!(
        !result.status.success(),
        "invalid option must be refused in preview too"
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("threads"));
    assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn unreadable_requested_companions_are_reported_before_audio_is_written() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new("unreadable_extras");
    let source = fixture.audio("Album/01.flac");
    fixture.scan();
    let extras = source.parent().unwrap().join("spectrograms");
    std::fs::create_dir(&extras).unwrap();
    std::fs::write(extras.join("01.png"), b"requested companion").unwrap();
    let permissions = std::fs::metadata(&extras).unwrap().permissions();
    std::fs::set_permissions(&extras, std::fs::Permissions::from_mode(0o000)).unwrap();
    let inaccessible = std::fs::read_dir(&extras).is_err();
    let result = fixture.copy(&["--extras", "all", "--safe-names"]);
    std::fs::set_permissions(&extras, permissions).unwrap();
    if inaccessible {
        assert!(
            !result.status.success(),
            "an unreadable requested folder must be reported"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("spectrograms"));
        assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 0);
    } else {
        // Privileged Unix test users can read folders despite mode 000.
        assert!(result.status.success());
        assert!(fixture.output.join("Album/spectrograms/01.png").is_file());
    }
}

#[cfg(unix)]
#[test]
fn stale_catalog_special_sources_are_refused_even_in_preview() {
    let fixture = Fixture::new("special_source");
    let source = fixture.audio("Album/01.flac");
    fixture.scan();
    std::fs::remove_file(&source).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let result = fixture.copy(&["--dry-run", "--extras", "none", "--safe-names"]);
    assert!(
        !result.status.success(),
        "a FIFO must not be scheduled as an audio file"
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("ordinary file"));
    assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 0);
}

#[test]
fn dry_run_preserves_preexisting_files_without_probing_or_creating_folders() {
    let fixture = Fixture::new("dry_run");
    fixture.audio("Album/01.flac");
    fixture.scan();
    let sentinel = fixture.output.join("aede-name-probe?.tmp");
    let sentinel = if std::fs::write(&sentinel, b"user content").is_ok() {
        sentinel
    } else {
        let sentinel = fixture.output.join("user-content.txt");
        std::fs::write(&sentinel, b"user content").unwrap();
        sentinel
    };
    let result = fixture.copy(&["--dry-run", "--extras", "none", "--playlists"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"user content");
    assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn a_destination_subfolder_link_cannot_replace_library_audio() {
    let fixture = Fixture::new("parent_link");
    let source = fixture.audio("Album/01.flac");
    fixture.scan();
    let original = std::fs::read(&source).unwrap();
    std::os::unix::fs::symlink(fixture.music.join("Album"), fixture.output.join("Album")).unwrap();
    let result = fixture.copy(&["--replace", "--extras", "none"]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("symbolic links"));
    assert_eq!(std::fs::read(source).unwrap(), original);
}

#[test]
fn existing_byte_verification_preserves_mismatches_until_explicit_replacement() {
    let fixture = Fixture::new("existing");
    let source = fixture.audio("Album/01.flac");
    fixture.scan();
    let target = fixture.output.join("Album/01.flac");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"different-size existing output").unwrap();
    assert!(!fixture.copy(&["--extras", "none"]).status.success());
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"different-size existing output"
    );
    assert!(
        fixture
            .copy(&["--extras", "none", "--replace"])
            .status
            .success()
    );
    assert!(fixture.copy(&["--extras", "none"]).status.success());
    assert!(
        fixture
            .copy(&["--extras", "none", "--verify-existing"])
            .status
            .success()
    );
    let mut wrong = std::fs::read(&target).unwrap();
    wrong[0] ^= 1;
    std::fs::write(&target, &wrong).unwrap();
    let result = fixture.copy(&["--extras", "none", "--verify-existing"]);
    assert!(!result.status.success());
    assert_eq!(std::fs::read(&target).unwrap(), wrong);
    assert!(
        fixture
            .copy(&["--extras", "none", "--verify-existing", "--replace"])
            .status
            .success()
    );
    assert_eq!(
        std::fs::read(&target).unwrap(),
        std::fs::read(source).unwrap()
    );
    assert_eq!(
        std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
        1,
        "all private temporary outputs were removed"
    );
}

#[test]
fn destination_playlists_name_the_adapted_converted_audio() {
    if aede_core::copy::transcode::find_ffmpeg().is_none() {
        assert!(
            std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none(),
            "destination conversion regression requires ffmpeg"
        );
        eprintln!("destination conversion test needs ffmpeg");
        return;
    }
    let fixture = Fixture::new("playlists");
    // '?' is not a valid source component on Windows, so exercise the safe
    // ASCII layout there while preserving the same conversion assertions.
    let relative = if cfg!(windows) {
        "Album/01.flac"
    } else {
        "Album?/01?.flac"
    };
    let source = fixture.audio(relative);
    let sidecar = source.parent().unwrap().join("album.m3u");
    let original = format!(
        "#EXTM3U\n#EXTINF:1,Track\n{}\nmissing.flac\n",
        source.file_name().unwrap().to_string_lossy()
    );
    std::fs::write(&sidecar, &original).unwrap();
    fixture.scan();
    let result = fixture.copy(&[
        "--extras",
        "all",
        "--safe-names",
        "--compress",
        "mp3",
        "--quality",
        "V0",
        "--playlists",
        "--verify",
    ]);
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let folder = if cfg!(windows) { "Album" } else { "Album_" };
    let name = if cfg!(windows) { "01.mp3" } else { "01_.mp3" };
    let target = fixture.output.join(folder).join(name);
    assert!(target.is_file());
    let selection = std::fs::read_to_string(fixture.output.join("aede-selection.m3u8")).unwrap();
    assert!(
        selection
            .lines()
            .any(|line| line == format!("{folder}/{name}"))
    );
    let copied_sidecar =
        std::fs::read_to_string(fixture.output.join(folder).join("album.m3u")).unwrap();
    assert!(copied_sidecar.lines().any(|line| line == name));
    assert!(!copied_sidecar.contains("missing.flac"));
    assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), original);
    let check = fixture.copy(&[
        "--extras",
        "none",
        "--safe-names",
        "--compress",
        "mp3",
        "--quality",
        "V0",
        "--verify-existing",
    ]);
    assert!(
        check.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    let wrong = b"an unrelated existing converted output";
    std::fs::write(&target, wrong).unwrap();
    let mismatch = fixture.copy(&[
        "--extras",
        "none",
        "--safe-names",
        "--compress",
        "mp3",
        "--quality",
        "V0",
        "--verify-existing",
    ]);
    assert!(!mismatch.status.success());
    assert_eq!(std::fs::read(&target).unwrap(), wrong);
    std::fs::write(&target, b"").unwrap();
    let empty = fixture.copy(&["--extras", "none", "--safe-names", "--compress", "mp3"]);
    assert!(!empty.status.success());
    assert_eq!(std::fs::read(&target).unwrap(), b"");
    let replaced = fixture.copy(&[
        "--extras",
        "none",
        "--safe-names",
        "--compress",
        "mp3",
        "--replace",
    ]);
    assert!(replaced.status.success());
    assert!(std::fs::metadata(&target).unwrap().len() > 0);
}

#[test]
fn composed_and_decomposed_album_names_copy_both_files_without_merging() {
    let fixture = Fixture::new("unicode");
    fixture.audio("one/Café/01.flac");
    fixture.audio("two/Cafe\u{301}/01.flac");
    let first = fixture.music.join("one");
    let second = fixture.music.join("two");
    let result = fixture.run(&["scan", first.to_str().unwrap(), second.to_str().unwrap()]);
    assert!(result.status.success());
    let result = fixture.copy(&["--extras", "none", "--safe-names"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let folders: Vec<_> = std::fs::read_dir(&fixture.output)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(folders.len(), 2, "two albums must have distinct folders");
    assert!(
        folders
            .iter()
            .all(|folder| folder.join("01.flac").is_file())
    );
}
