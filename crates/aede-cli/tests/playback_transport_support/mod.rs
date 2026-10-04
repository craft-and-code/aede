use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use aede_core::{conclusions, model, store, tags, user};

pub struct Library {
    pub root: PathBuf,
    runs: usize,
    genres: BTreeMap<PathBuf, Vec<String>>,
}

pub struct Playback {
    pub output: Output,
    pub pcm: Vec<u8>,
    pub data: PathBuf,
}

impl Library {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "aede_playback_transport_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).expect("isolated transport library");
        let bin = root.join("bin");
        std::fs::create_dir(&bin).expect("fake output directory");
        let ffplay = bin.join("ffplay");
        std::fs::write(&ffplay, "#!/bin/sh\ncat >> \"$AEDE_TEST_PCM\"\n").expect("fake output");
        std::fs::set_permissions(ffplay, std::fs::Permissions::from_mode(0o700))
            .expect("executable fake output");
        Self {
            root,
            runs: 0,
            genres: BTreeMap::new(),
        }
    }

    pub fn wav(&self, name: &str, frames: usize, value: i16) -> PathBuf {
        self.write_wav(name, frames, |frame| {
            value.saturating_add((frame % 101) as i16)
        })
    }

    pub fn timeline(&self, name: &str, seconds: usize) -> PathBuf {
        self.write_wav(name, seconds * 48_000, |frame| {
            2_000 + (frame / 48_000) as i16 * 1_000
        })
    }

    fn write_wav(&self, name: &str, frames: usize, sample: impl Fn(usize) -> i16) -> PathBuf {
        let path = self.root.join(name);
        let payload_len = u32::try_from(frames * 2).expect("small synthetic WAV");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + payload_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&payload_len.to_le_bytes());
        for frame in 0..frames {
            bytes.extend_from_slice(&sample(frame).to_le_bytes());
        }
        std::fs::write(&path, bytes).expect("synthetic WAV");
        path
    }

    pub fn copy_core_fixture(&self, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(if name.starts_with("playback") {
                "../aede-core/tests/playback_fixtures/flac"
            } else {
                "../aede-core/tests/fixtures"
            })
            .join(name);
        let path = self.root.join(name);
        std::fs::copy(source, &path).expect("disposable audio fixture");
        path
    }

    pub fn flac_with_wrong_audio_md5(&self, valid: &Path) -> PathBuf {
        let path = self.root.join("wrong-audio-md5.flac");
        let mut bytes = std::fs::read(valid).expect("real FLAC fixture");
        assert_eq!(&bytes[..4], b"fLaC");
        assert_eq!(bytes[4] & 0x7f, 0, "first block is STREAMINFO");
        assert_eq!(&bytes[5..8], &[0, 0, 34]);
        assert!(bytes[26..42].iter().any(|byte| *byte != 0));
        // Only STREAMINFO's decoded-audio MD5 changes. Encoded frames, their
        // CRCs and the sample programme remain the original valid fixture.
        bytes[26] ^= 1;
        std::fs::write(&path, bytes).expect("disposable MD5 mismatch");
        path
    }

    pub fn set_genres(&mut self, path: &Path, genres: &[&str]) {
        self.genres.insert(
            path.to_path_buf(),
            genres.iter().map(|genre| (*genre).to_owned()).collect(),
        );
    }

    pub fn play(&mut self, paths: &[&Path], options: &[&str], catalog: bool) -> Playback {
        let run = self.root.join(format!("run_{}", self.runs));
        self.runs += 1;
        std::fs::create_dir(&run).expect("isolated playback run");
        let playlist = run.join("selection.m3u");
        let entries = paths
            .iter()
            .map(|path| format!("{}\n", path.display()))
            .collect::<String>();
        std::fs::write(&playlist, entries).expect("selection with preserved duplicates");
        let data = run.join("data");
        if catalog {
            self.catalog(&data, paths);
        }
        let pcm = run.join("output.pcm");
        let mut command = self.command(&data, &pcm);
        command.arg("play").arg(playlist).args(options);
        let stdout = run.join("stdout");
        let stderr = run.join("stderr");
        command
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&stdout).expect("stdout capture"))
            .stderr(std::fs::File::create(&stderr).expect("stderr capture"));
        let mut child = command.spawn().expect("CLI playback");
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.try_wait().expect("playback status") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "playback timed out: {}",
                    String::from_utf8_lossy(&std::fs::read(&stderr).unwrap_or_default())
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        Playback {
            output: Output {
                status,
                stdout: std::fs::read(stdout).expect("captured stdout"),
                stderr: std::fs::read(stderr).expect("captured stderr"),
            },
            pcm: std::fs::read(pcm).unwrap_or_default(),
            data,
        }
    }

    pub fn terminal(&mut self, paths: &[&Path], scenario: &str) -> Playback {
        let run = self.root.join(format!("terminal_{}", self.runs));
        self.runs += 1;
        std::fs::create_dir(&run).expect("isolated terminal run");
        let playlist = run.join("selection.m3u");
        let entries = paths
            .iter()
            .map(|path| format!("{}\n", path.display()))
            .collect::<String>();
        std::fs::write(&playlist, entries).expect("terminal selection");
        if scenario == "paused-modes" {
            self.catalog(&run.join("data"), paths);
        }
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/playback_transport_support/terminal.py");
        let output = Command::new("python3")
            .arg(script)
            .arg(env!("CARGO_BIN_EXE_aede"))
            .arg(&run)
            .arg(playlist)
            .arg(scenario)
            .output()
            .expect("stdlib pseudo-terminal playback probe");
        Playback {
            output,
            pcm: std::fs::read(run.join("output.pcm")).unwrap_or_default(),
            data: run.join("data"),
        }
    }

    fn command(&self, data: &Path, pcm: &Path) -> Command {
        let mut search_path = OsString::from(self.root.join("bin"));
        search_path.push(":");
        search_path.push(std::env::var_os("PATH").unwrap_or_default());
        let mut command = Command::new(env!("CARGO_BIN_EXE_aede"));
        command
            .arg("--data")
            .arg(data)
            .env("PATH", search_path)
            .env("AEDE_AUDIO_BACKEND", "ffplay")
            .env("AEDE_TEST_PCM", pcm)
            .env("NO_COLOR", "1")
            .env_remove("AEDE_DELEGATED_CHILD")
            .env_remove("AEDE_DELEGATED_DATA_DIR");
        command
    }

    fn catalog(&self, data: &Path, paths: &[&Path]) {
        let mut unique = paths.to_vec();
        unique.sort();
        unique.dedup();
        let scanned = unique
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let mut tags = tags::read(path).expect("synthetic stream properties");
                for (key, values) in [
                    ("title", vec![format!("Selection {index}")]),
                    ("artist", vec![format!("Performer {index}")]),
                    ("albumartist", vec![format!("Performer {index}")]),
                    ("album", vec!["Connected selection".into()]),
                    ("genre", vec!["Rock".into(), "Pop".into()]),
                ] {
                    tags.fields.insert(key.into(), values);
                }
                if let Some(genres) = self.genres.get(*path) {
                    tags.fields.insert("genre".into(), genres.clone());
                }
                let metadata = std::fs::metadata(path).expect("fixture identity");
                model::ScannedFile {
                    path: path
                        .canonicalize()
                        .expect("fixture canonical path")
                        .to_string_lossy()
                        .into_owned(),
                    size: metadata.len(),
                    mtime: aede_core::clock::mtime_seconds(&metadata),
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }
            })
            .collect();
        let catalog = model::build(scanned, Vec::new(), 0, &[]);
        store::save_catalog_only(&catalog, &store::catalog_path(data)).expect("test catalog");
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl Playback {
    pub fn assert_success(&self) {
        assert!(
            self.output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr)
        );
    }

    pub fn history(&self) -> Option<user::UserData> {
        user::load(&user::user_path(&self.data)).expect("isolated history")
    }

    pub fn assert_no_loudness_cache(&self) {
        if let Some(data) = conclusions::load(&conclusions::conclusions_path(&self.data))
            .expect("isolated conclusions")
        {
            assert!(
                data.loudness_tracks.is_empty(),
                "a suffix cannot be measured as a full track"
            );
            assert!(
                data.loudness_programmes.is_empty(),
                "a suffix cannot complete an album programme"
            );
        }
    }
}

pub fn pcm(path: &Path) -> Vec<u8> {
    let mut track = aede_core::playback::stream::PcmTrack::open(path).expect("reference source");
    let mut output = Vec::new();
    while let Some(block) = track
        .read_block(|_| Ok::<(), std::convert::Infallible>(()))
        .expect("reference PCM")
    {
        output.extend_from_slice(block.f32le);
    }
    output
}
