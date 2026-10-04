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
