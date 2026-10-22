use super::*;

#[cfg(unix)]
#[test]
fn loudness_identity_refuses_paths_that_cannot_be_persisted_without_collisions() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let directory = crate::store_lock::test_support::Directory::new("loudness_utf8");
    let path = directory.path().join(OsStr::from_bytes(b"track-\xff.flac"));
    // APFS itself rejects these filenames. Refuse their lossy persisted key
    // before touching the filesystem, on every Unix host.
    let error = identity(&path).expect_err("identity needs an exact persisted path");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.to_string().contains("UTF-8"));
}

fn wave(path: &Path, rate: u32, amplitude: f32) {
    let frames = rate;
    let bytes = frames * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    for index in 0..frames {
        let sample = ((std::f32::consts::TAU * 1000.0 * index as f32 / rate as f32).sin()
            * amplitude
            * i16::MAX as f32) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, wav).unwrap();
}

fn decodable_silent_wave(path: &Path, rate: u32) -> Vec<u8> {
    wave(path, rate, 0.0);
    let original = std::fs::read(path).unwrap();
    let mut decoder = FileDecoder::open(path).expect("valid mono PCM16 WAV");
    assert_eq!(decoder.sample_rate(), rate);
    assert_eq!(decoder.channels(), 1);
    let mut buffer = [1.0; 4096];
    let mut decoded_frames = 0;
    loop {
        let frames = decoder.read_frames(&mut buffer).unwrap();
        if frames == 0 {
            break;
        }
        assert!(buffer[..frames].iter().all(|sample| *sample == 0.0));
        decoded_frames += frames;
    }
    assert_eq!(decoded_frames, rate as usize);
    original
}

fn assert_low_rate_track_refused(rate: u32) {
    let directory = crate::store_lock::test_support::Directory::new("loudness_track_rate");
    let path = directory.path().join("silence.wav");
    let original = decodable_silent_wave(&path, rate);
    assert!(
        measure_track(&path).is_err(),
        "{rate} Hz must return a loudness measurement error"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

fn assert_low_rate_programme_refused(rate: u32) {
    let directory = crate::store_lock::test_support::Directory::new("loudness_programme_rate");
    let path = directory.path().join("silence.wav");
    let original = decodable_silent_wave(&path, rate);
    assert!(
        measure_programme(&[path.as_path()]).is_err(),
        "{rate} Hz must return a programme measurement error"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn track_loudness_refuses_real_pcm16_wav_at_16_hz() {
    assert_low_rate_track_refused(16);
}

#[test]
fn track_loudness_refuses_real_pcm16_wav_at_17_hz() {
    assert_low_rate_track_refused(17);
}

#[test]
fn programme_loudness_refuses_real_pcm16_wav_at_16_hz() {
    assert_low_rate_programme_refused(16);
}

#[test]
fn programme_loudness_refuses_real_pcm16_wav_at_17_hz() {
    assert_low_rate_programme_refused(17);
}

#[test]
fn loudness_refuses_real_pcm16_wav_below_16_hz() {
    for rate in [1, 15] {
        assert_low_rate_track_refused(rate);
        assert_low_rate_programme_refused(rate);
    }
}

#[test]
fn programme_loudness_refuses_real_low_rate_wav_after_valid_audio() {
    for rate in [16, 17] {
        let directory = crate::store_lock::test_support::Directory::new("loudness_rate_change");
        let valid = directory.path().join("valid.wav");
        wave(&valid, 44_100, 0.25);
        let valid_original = std::fs::read(&valid).unwrap();
        assert!(measure_programme(&[valid.as_path()]).unwrap().is_some());
        let unsupported = directory.path().join("unsupported.wav");
        let unsupported_original = decodable_silent_wave(&unsupported, rate);
        assert!(
            measure_programme(&[valid.as_path(), unsupported.as_path()]).is_err(),
            "a {rate} Hz section must return a programme measurement error"
        );
        assert_eq!(std::fs::read(&valid).unwrap(), valid_original);
        assert_eq!(std::fs::read(&unsupported).unwrap(), unsupported_original);
    }
}

#[test]
fn loudness_accepts_real_pcm16_silence_at_18_hz_and_conventional_rates() {
    for rate in [18, 8_000, 44_100] {
        let directory = crate::store_lock::test_support::Directory::new("loudness_silent_rate");
        let path = directory.path().join("silence.wav");
        let original = decodable_silent_wave(&path, rate);
        assert_eq!(measure_track(&path).unwrap(), None);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(measure_programme(&[path.as_path()]).unwrap(), None);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}

#[test]
fn imported_loudness_requires_current_bytes_and_attributed_success() {
    let file = ProgrammeFile {
        path: "/music/a.flac".into(),
        size: 42,
        mtime: 7,
        mtime_subseconds: None,
    };
    let valid = FileAnalysis {
        path: file.path.clone(),
        source: "flaccompagnon".into(),
        size_bytes: 42,
        modified_unix: 7,
        integrated_lufs: Some(-16.0),
        true_peak_dbtp: Some(-3.0),
        ..Default::default()
    };
    let found = from_flaccompagnon(std::slice::from_ref(&valid), &file).unwrap();
    assert_eq!(found.integrated_lufs, -16.0);
    assert!((found.true_peak.unwrap() - 0.708).abs() < 0.002);
    let mut no_peak = valid.clone();
    no_peak.true_peak_dbtp = None;
    no_peak.peak_dbfs = Some(0.0);
    assert_eq!(
        from_flaccompagnon(&[no_peak], &file).unwrap().true_peak,
        None
    );
    let mut stale = valid.clone();
    stale.modified_unix += 1;
    assert_eq!(from_flaccompagnon(&[stale], &file), None);
    let mut failed = valid.clone();
    failed.error = Some("decode failed".into());
    assert_eq!(from_flaccompagnon(&[failed], &file), None);
    let mut wrong_source = valid;
    wrong_source.source = "unknown".into();
    assert_eq!(from_flaccompagnon(&[wrong_source], &file), None);
}

#[test]
fn imported_true_peak_cannot_justify_gain_above_a_known_sample_peak() {
    let file = ProgrammeFile {
        path: "/music/last-frame.flac".into(),
        size: 42,
        mtime: 7,
        mtime_subseconds: Some(1),
    };
    // An unfinished interpolation tail can miss a final-frame sample peak.
    // Keep the external evidence intact while deriving safer playback headroom.
    for (sample_dbfs, true_dbtp, expected_db) in
        [(0.0, -68.8, 0.0), (-3.0, -20.0, -3.0), (-3.0, -1.0, -1.0)]
    {
        let analysis = FileAnalysis {
            path: file.path.clone(),
            source: "flaccompagnon".into(),
            size_bytes: file.size,
            modified_unix: file.mtime,
            integrated_lufs: Some(-36.0),
            peak_dbfs: Some(sample_dbfs),
            true_peak_dbtp: Some(true_dbtp),
            ..Default::default()
        };
        let original = analysis.clone();
        let found = from_flaccompagnon(std::slice::from_ref(&analysis), &file).unwrap();
        let gain = aede_dsp::gain_with_headroom_db(18.0, found.true_peak).unwrap();
        assert!((f64::from(gain) + expected_db).abs() < 0.0001);
        assert_eq!(
            analysis, original,
            "source measurements retain their provenance"
        );
    }
}

#[test]
fn album_loudness_is_gated_as_a_programme_even_across_sample_rates() {
    let root = std::env::temp_dir().join(format!(
        "aede_loudness_{}_{}",
        std::process::id(),
        crate::clock::now_seconds()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let loud = root.join("loud.wav");
    let quiet = root.join("quiet.wav");
    wave(&loud, 48_000, 0.5);
    wave(&quiet, 44_100, 0.05);
    let first = measure_track(&loud).unwrap().unwrap();
    let second = measure_track(&quiet).unwrap().unwrap();
    let album = measure_programme(&[loud.as_path(), quiet.as_path()])
        .unwrap()
        .unwrap();
    assert!(album.integrated_lufs <= first.integrated_lufs + 0.2);
    assert!(album.integrated_lufs > second.integrated_lufs);
    assert!(
        (album.integrated_lufs - (first.integrated_lufs + second.integrated_lufs) / 2.0).abs()
            > 2.0
    );
    assert!(album.true_peak.unwrap() >= first.true_peak.unwrap() * 0.99);
    std::fs::remove_dir_all(root).unwrap();
}
