#![cfg(unix)]

use aede_core::user;

mod fixtures {
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::atomic::{AtomicU64, Ordering};

    use aede_core::user;

    pub struct TestLibrary {
        pub root: PathBuf,
        runs: usize,
    }

    pub struct Playback {
        pub result: Output,
        pub pcm: Vec<u8>,
        pub streams: usize,
        pub history: Option<user::UserData>,
    }

    impl TestLibrary {
        pub fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "aede_cli_continuity_{}_{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).expect("temporary library");
            let bin = root.join("bin");
            std::fs::create_dir(&bin).expect("output stub directory");
            let ffplay = bin.join("ffplay");
            std::fs::write(
                &ffplay,
                "#!/bin/sh\nprintf x >> \"$AEDE_TEST_STREAMS\"\ncat >> \"$AEDE_TEST_PCM\"\n",
            )
            .expect("output stub");
            std::fs::set_permissions(&ffplay, std::fs::Permissions::from_mode(0o700))
                .expect("executable output stub");
            Self { root, runs: 0 }
        }

        pub fn wav(&self, name: &str, rate: u32, channels: u16, frames: usize) -> PathBuf {
            let path = self.root.join(name);
            let payload_len =
                u32::try_from(frames * usize::from(channels) * 2).expect("small fixture payload");
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(36 + payload_len).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt ");
            bytes.extend_from_slice(&16u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&channels.to_le_bytes());
            bytes.extend_from_slice(&rate.to_le_bytes());
            bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
            bytes.extend_from_slice(&(channels * 2).to_le_bytes());
            bytes.extend_from_slice(&16u16.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            for _ in 0..frames {
                for channel in 0..channels {
                    let sample = if channel == 0 { 8192i16 } else { -4096i16 };
                    bytes.extend_from_slice(&sample.to_le_bytes());
                }
            }
            std::fs::write(&path, bytes).expect("generated WAV");
            path
        }

        pub fn play(&mut self, paths: &[&Path]) -> Playback {
            let run = self.root.join(format!("run_{}", self.runs));
            self.runs += 1;
            std::fs::create_dir(&run).expect("isolated playback run");
            let playlist = run.join("play.m3u");
            let entries = paths
                .iter()
                .map(|path| format!("{}\n", path.display()))
                .collect::<String>();
            std::fs::write(&playlist, entries).expect("playlist");
            let pcm_path = run.join("output.pcm");
            let streams_path = run.join("streams");
            let data = run.join("data");
            let mut search_path = OsString::from(self.root.join("bin").as_os_str());
            search_path.push(":");
            search_path.push(std::env::var_os("PATH").unwrap_or_default());
            let result = Command::new(env!("CARGO_BIN_EXE_aede"))
                .arg("play")
                .arg(&playlist)
                .args(["--normalize", "off", "--bass", "6", "--treble", "-3"])
                .arg("--data")
                .arg(&data)
                .env("PATH", search_path)
                .env("AEDE_AUDIO_BACKEND", "ffplay")
                .env("AEDE_TEST_PCM", &pcm_path)
                .env("AEDE_TEST_STREAMS", &streams_path)
                .output()
                .expect("CLI playback");
            let pcm = std::fs::read(pcm_path).unwrap_or_default();
            let streams = std::fs::read(streams_path).unwrap_or_default().len();
            let history = user::load(&user::user_path(&data)).expect("read listening history");
            Playback {
                result,
                pcm,
                streams,
                history,
            }
        }

        pub fn wav_f32(&self, name: &str, rate: u32, channels: u16, samples: &[f32]) -> PathBuf {
            assert!(samples.len().is_multiple_of(usize::from(channels)));
            let path = self.root.join(name);
            let payload_len = u32::try_from(samples.len() * 4).expect("small float WAV");
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(36 + payload_len).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt ");
            bytes.extend_from_slice(&16u32.to_le_bytes());
            bytes.extend_from_slice(&3u16.to_le_bytes());
            bytes.extend_from_slice(&channels.to_le_bytes());
            bytes.extend_from_slice(&rate.to_le_bytes());
            bytes.extend_from_slice(&(rate * u32::from(channels) * 4).to_le_bytes());
            bytes.extend_from_slice(&(channels * 4).to_le_bytes());
            bytes.extend_from_slice(&32u16.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            for sample in samples {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            std::fs::write(&path, bytes).expect("generated float WAV");
            path
        }
    }

    impl Drop for TestLibrary {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    pub fn assert_success(playback: &Playback) {
        assert!(
            playback.result.status.success(),
            "{}",
            String::from_utf8_lossy(&playback.result.stderr)
        );
        assert!(!playback.pcm.is_empty(), "decoded audio reaches the sink");
    }

    pub fn assert_pcm_equal(expected: &[u8], actual: &[u8]) {
        assert_eq!(actual.len(), expected.len(), "submitted PCM byte count");
        assert_eq!(actual.len() % 4, 0, "complete f32 PCM samples");
        let mismatch = actual.iter().zip(expected).position(|(a, b)| a != b);
        assert_eq!(mismatch, None, "first differing PCM byte");
        assert!(actual.as_chunks::<4>().0.iter().all(|bytes| {
            let sample = f32::from_le_bytes(*bytes);
            sample.is_finite() && sample.abs() <= 1.0
        }));
    }
}

use fixtures::{TestLibrary, assert_pcm_equal, assert_success};

fn assert_history(history: &user::UserData, files: &[(&std::path::Path, u64)]) {
    assert_eq!(history.plays.len(), files.len());
    assert_eq!(history.counts.len(), files.len());
    for (path, duration_ms) in files {
        let key = path
            .canonicalize()
            .expect("fixture identity")
            .to_string_lossy()
            .into_owned();
        let plays = history
            .plays
            .iter()
            .filter(|play| play.track.key == key)
            .collect::<Vec<_>>();
        assert_eq!(plays.len(), 1, "one listening event per source file");
        assert_eq!(plays[0].owner, user::LOCAL_USER);
        assert!(
            plays[0].completed,
            "complete decoding remains a complete listen"
        );
        assert_eq!(
            plays[0].ms_played, *duration_ms,
            "duration belongs to its source file"
        );
        let count = history
            .counts
            .iter()
            .find(|count| count.track == plays[0].track)
            .expect("matching all-time counter");
        assert_eq!(count.owner, user::LOCAL_USER);
        assert_eq!(count.count, 1);
    }
}

#[test]
fn split_files_keep_the_same_tone_processed_pcm_as_one_continuous_file() {
    let mut library = TestLibrary::new();
    let frames = 10_001;
    let rate = 44_100;
    let whole = library.wav("whole.wav", rate, 2, frames * 2);
    let first = library.wav("first.wav", rate, 2, frames);
    let second = library.wav("second.wav", rate, 2, frames);
    let reference = library.play(&[&whole]);
    assert_success(&reference);
    let split = library.play(&[&first, &second]);
    assert_success(&split);
    assert_eq!(reference.streams, 1);
    assert_eq!(split.streams, 1);
    assert_eq!(split.pcm.len(), frames * 2 * 2 * 4);
    assert_history(
        reference.history.as_ref().expect("whole-file history"),
        &[(&whole, (frames * 2) as u64 * 1000 / u64::from(rate))],
    );
    let duration = frames as u64 * 1000 / u64::from(rate);
    assert_history(
        split.history.as_ref().expect("split-file history"),
        &[(&first, duration), (&second, duration)],
    );
    assert_pcm_equal(&reference.pcm, &split.pcm);
}

#[test]
fn incompatible_source_rates_and_channels_start_fresh_tone_processing() {
    let mut library = TestLibrary::new();
    let first = library.wav("stereo_44100.wav", 44_100, 2, 10_001);
    let second = library.wav("stereo_48000.wav", 48_000, 2, 10_003);
    let third = library.wav("mono_48000.wav", 48_000, 1, 10_007);
    let mut expected = Vec::new();
    for path in [&first, &second, &third] {
        let single = library.play(&[path]);
        assert_success(&single);
        expected.extend_from_slice(&single.pcm);
    }
    let sequence = library.play(&[&first, &second, &third]);
    assert_success(&sequence);
    assert_eq!(sequence.streams, 3);
    assert_history(
        sequence.history.as_ref().expect("format-change history"),
        &[
            (&first, 10_001 * 1000 / 44_100),
            (&second, 10_003 * 1000 / 48_000),
            (&third, 10_007 * 1000 / 48_000),
        ],
    );
    assert_pcm_equal(&expected, &sequence.pcm);
}

#[test]
fn a_corrupt_later_file_preserves_the_first_processed_output_and_complete_listen() {
    let mut library = TestLibrary::new();
    let first = library.wav("first.wav", 44_100, 2, 10_001);
    let corrupt = library.root.join("corrupt.wav");
    std::fs::write(&corrupt, b"not a decodable WAV").expect("invalid later source");
    let reference = library.play(&[&first]);
    assert_success(&reference);
    let attempted = library.play(&[&first, &corrupt]);
    assert!(
        !attempted.result.status.success(),
        "later corruption is reported"
    );
    assert_eq!(attempted.streams, 1);
    assert_history(
        attempted
            .history
            .as_ref()
            .expect("completed first-file history"),
        &[(&first, 10_001 * 1000 / 44_100)],
    );
    assert_pcm_equal(&reference.pcm, &attempted.pcm);
}

#[test]
fn vorbis_joins_keep_exact_frames_continuous_tone_and_each_listening_identity() {
    use aede_core::playback::decoder::FileDecoder;
    let mut library = TestLibrary::new();
    let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/playback_fixtures");
    let first = fixtures.join("vorbis-stereo-10001.ogg");
    let second = fixtures.join("vorbis-stereo-300013.ogg");
    let mut decoded = Vec::new();
    for path in [&first, &second] {
        let mut decoder = FileDecoder::open(path).expect("native Vorbis opens");
        let mut buffer = vec![0.0; 127 * 2];
        loop {
            let frames = decoder.read_frames(&mut buffer).expect("Vorbis frames");
            if frames == 0 {
                break;
            }
            decoded.extend_from_slice(&buffer[..frames * 2]);
        }
    }
    assert_eq!(decoded.len(), 310_014 * 2);
    let whole = library.wav_f32("whole.wav", 48_000, 2, &decoded);
    let reference = library.play(&[&whole]);
    assert_success(&reference);
    let split = library.play(&[&first, &second]);
    assert_success(&split);
    assert_eq!(split.streams, 1);
    assert_eq!(split.pcm.len(), 310_014 * 2 * 4);
    assert_pcm_equal(&reference.pcm, &split.pcm);
    assert_history(
        split.history.as_ref().expect("Vorbis history"),
        &[
            (&first, 10_001 * 1000 / 48_000),
            (&second, 300_013 * 1000 / 48_000),
        ],
    );
}
