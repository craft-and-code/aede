use super::*;
use crate::analysis::FileAnalysis;
use std::sync::atomic::{AtomicU64, Ordering};

fn root() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aede_ready_gain_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).expect("directory");
    root
}

fn fixture(root: &Path, name: &str) -> PathBuf {
    fixture_from(root, name, "playback-stereo.flac")
}

fn fixture_from(root: &Path, name: &str, source_name: &str) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(if source_name.starts_with("playback") {
            "tests/playback_fixtures/flac"
        } else {
            "tests/fixtures"
        })
        .join(source_name);
    let target = root.join(name);
    std::fs::copy(source, &target).expect("fixture copy");
    target
}

fn surround_fixture(directory: &Path) -> PathBuf {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    let target = directory.join("surround.wav");
    std::fs::copy(source, &target).expect("surround fixture copy");
    target
}

fn observe_unsupported_file(session: &mut ReadyNormalization<'_>, path: &Path) {
    let mut decoder = crate::playback::decoder::FileDecoder::open(path).expect("decoder");
    let format = PcmFormat::new(decoder.sample_rate(), decoder.channels()).expect("format");
    let mut samples = vec![0.0; 4096 * usize::from(format.channels())];
    let frames = decoder.read_frames(&mut samples).expect("decode");
    let samples = &samples[..frames * usize::from(format.channels())];
    assert!(!samples.is_empty());
    assert!(session.observe_source(format, samples).is_err());
    assert!(!session.is_measuring());
    session
        .observe_source(format, samples)
        .expect("no repeated warning");
}

fn fresh_analysis(path: &Path) -> FileAnalysis {
    let file = loudness::identity(path).expect("file identity");
    FileAnalysis {
        path: file.path,
        source: "flaccompagnon".into(),
        size_bytes: file.size,
        modified_unix: file.mtime,
        integrated_lufs: Some(-16.0),
        true_peak_dbtp: Some(-3.0),
        ..Default::default()
    }
}

fn save_analysis(directory: &Path, analysis: FileAnalysis) {
    let cache = Conclusions {
        analyses: vec![analysis],
        ..Default::default()
    };
    conclusions::save(&cache, &conclusions::conclusions_path(directory)).expect("analysis cache");
}

fn append_source_byte(path: &Path) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open source copy")
        .write_all(&[0])
        .expect("change source size");
}

fn replaygain_fixture(directory: &Path) -> PathBuf {
    let path = fixture(directory, "tagged.flac");
    let original = std::fs::read(&path).expect("FLAC bytes");
    assert_eq!(&original[..4], b"fLaC");
    let mut bytes = original[..4].to_vec();
    let mut offset = 4;
    loop {
        let kind = original[offset] & 0x7f;
        let last = original[offset] & 0x80 != 0;
        let size = usize::from(original[offset + 1]) << 16
            | usize::from(original[offset + 2]) << 8
            | usize::from(original[offset + 3]);
        let end = offset + 4 + size;
        if kind != 4 {
            bytes.push(kind);
            bytes.extend_from_slice(&original[offset + 1..end]);
        }
        offset = end;
        if last {
            break;
        }
    }
    let entries = [
        "REPLAYGAIN_TRACK_GAIN=-4.0 dB",
        "REPLAYGAIN_TRACK_PEAK=0.8",
        "REPLAYGAIN_ALBUM_GAIN=-6.0 dB",
        "REPLAYGAIN_ALBUM_PEAK=0.9",
    ];
    let mut comment = Vec::new();
    comment.extend_from_slice(&0u32.to_le_bytes());
    comment.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for entry in entries {
        comment.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        comment.extend_from_slice(entry.as_bytes());
    }
    bytes.push(0x84);
    bytes.extend_from_slice(&(comment.len() as u32).to_be_bytes()[1..]);
    bytes.extend_from_slice(&comment);
    bytes.extend_from_slice(&original[offset..]);
    std::fs::write(&path, bytes).expect("tagged fixture copy");
    path
}

fn observe_file(session: &mut ReadyNormalization<'_>, path: &Path) {
    let mut decoder = crate::playback::decoder::FileDecoder::open(path).expect("decoder");
    let format =
        aede_dsp::PcmFormat::new(decoder.sample_rate(), decoder.channels()).expect("format");
    let mut samples = vec![0.0; 4096 * usize::from(format.channels())];
    loop {
        let frames = decoder.read_frames(&mut samples).expect("decode");
        if frames == 0 {
            break;
        }
        session
            .observe_source(format, &samples[..frames * usize::from(format.channels())])
            .expect("source meter");
    }
}

#[test]
fn playback_prepares_only_the_requested_track_and_never_predecodes_missing_loudness() {
    let directory = root();
    let first = fixture(&directory, "first.flac");
    let paths = vec![first, directory.join("missing-later.flac")];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    assert!(session.prepare(0).expect("first gain").is_none());
    assert!(session.is_measuring());
    assert!(!conclusions::conclusions_path(&directory).exists());
    assert!(session.prepare(1).is_err());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn source_measurement_is_cached_only_after_complete_unchanged_decoding() {
    let directory = root();
    let path = fixture(&directory, "first.flac");
    let paths = vec![path.clone()];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    session.prepare(0).expect("prepare");
    observe_file(&mut session, &path);
    assert!(session.finish_track(0, false).expect("skip").is_none());
    assert!(!conclusions::conclusions_path(&directory).exists());
    session.prepare(0).expect("restart");
    observe_file(&mut session, &path);
    let update = session
        .finish_track(0, true)
        .expect("finish")
        .expect("update");
    update.save(&directory).expect("cache save");
    let mut next =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("next session");
    assert_eq!(
        next.prepare(0).expect("cached gain").expect("gain").label,
        "measured track"
    );
    assert!(!next.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn missing_album_gain_stays_uniform_and_is_published_only_after_all_tracks() {
    let directory = root();
    let paths = vec![
        fixture(&directory, "first.flac"),
        fixture(&directory, "second.flac"),
    ];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("session");
    assert!(session.prepare(0).expect("prepare").is_none());
    observe_file(&mut session, &paths[0]);
    assert!(session.finish_track(0, true).expect("first end").is_none());
    assert!(session.prepare(1).expect("second gain").is_none());
    observe_file(&mut session, &paths[1]);
    let update = session
        .finish_track(1, true)
        .expect("album end")
        .expect("programme update");
    update.save(&directory).expect("save");
    let mut next =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("next session");
    let first = next.prepare(0).expect("first gain").expect("gain");
    let second = next.prepare(1).expect("second gain").expect("gain");
    assert_eq!(first.label, "measured album");
    assert_eq!(first.gain_db, second.gain_db);
    assert!(!next.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn normalization_off_does_not_inspect_files_or_cache() {
    let directory = root();
    let paths = vec![directory.join("not-there.flac")];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Off)
            .expect("session");
    assert!(session.prepare(0).expect("off").is_none());
    assert!(!session.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn fresh_flaccompagnon_loudness_is_reused_without_source_capture() {
    let directory = root();
    let path = fixture(&directory, "first.flac");
    let paths = vec![path.clone()];
    save_analysis(&directory, fresh_analysis(&path));
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    let plan = session.prepare(0).expect("prepare").expect("imported gain");
    assert_eq!(plan.label, "FlacCompagnon track");
    assert_eq!(plan.gain_db, -2.0);
    assert_eq!(plan.peak_label, "measured true peak");
    assert!((plan.source_peak.expect("true peak") - 0.708).abs() < 0.002);
    assert!(!session.is_measuring());
    assert!(session.finish_track(0, true).expect("finish").is_none());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn stale_failed_or_unusable_imports_start_source_capture() {
    let directory = root();
    let path = fixture(&directory, "first.flac");
    let paths = vec![path.clone()];
    let valid = fresh_analysis(&path);
    let mut wrong_size = valid.clone();
    wrong_size.size_bytes += 1;
    let mut wrong_time = valid.clone();
    wrong_time.modified_unix += 1;
    let mut failed = valid.clone();
    failed.error = Some("decode failed".into());
    let mut wrong_source = valid.clone();
    wrong_source.source = "unknown".into();
    let mut no_loudness = valid.clone();
    no_loudness.integrated_lufs = None;
    let mut invalid_loudness = valid;
    invalid_loudness.integrated_lufs = Some(21.0);
    for analysis in [
        wrong_size,
        wrong_time,
        failed,
        wrong_source,
        no_loudness,
        invalid_loudness,
    ] {
        save_analysis(&directory, analysis);
        let mut session =
            ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
                .expect("session");
        assert!(session.prepare(0).expect("prepare").is_none());
        assert!(session.is_measuring());
    }
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn exact_scope_tags_take_priority_over_fresh_imported_loudness() {
    let directory = root();
    let path = replaygain_fixture(&directory);
    let paths = vec![path.clone()];
    save_analysis(&directory, fresh_analysis(&path));
    for (mode, label, gain, peak) in [
        (NormalizationMode::Track, "ReplayGain track", -4.0, 0.8),
        (NormalizationMode::Album, "ReplayGain album", -6.0, 0.9),
    ] {
        let mut session =
            ReadyNormalization::new(&paths, false, None, &directory, mode).expect("session");
        let plan = session.prepare(0).expect("prepare").expect("tag gain");
        assert_eq!(plan.label, label);
        assert_eq!(plan.gain_db, gain);
        assert_eq!(plan.source_peak, Some(peak));
        assert!(!session.is_measuring());
    }
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn a_source_changed_after_capture_is_rejected_before_finishing() {
    let directory = root();
    let path = fixture(&directory, "first.flac");
    let paths = vec![path.clone()];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    session.prepare(0).expect("prepare");
    observe_file(&mut session, &path);
    append_source_byte(&path);
    let error = session
        .finish_track(0, true)
        .err()
        .expect("changed source rejected");
    assert!(error.to_string().contains("changed while"));
    assert!(!session.is_measuring());
    assert!(!conclusions::conclusions_path(&directory).exists());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn a_source_changed_between_finish_and_save_is_not_published() {
    let directory = root();
    let path = fixture(&directory, "first.flac");
    let paths = vec![path.clone()];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    session.prepare(0).expect("prepare");
    observe_file(&mut session, &path);
    let update = session
        .finish_track(0, true)
        .expect("finish")
        .expect("update");
    append_source_byte(&path);
    let error = update
        .save(&directory)
        .expect_err("changed source rejected");
    assert!(error.to_string().contains("changed before"));
    assert!(!conclusions::conclusions_path(&directory).exists());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn an_interrupted_album_never_publishes_a_partial_programme() {
    let directory = root();
    let paths = vec![
        fixture(&directory, "first.flac"),
        fixture(&directory, "second.flac"),
    ];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("session");
    session.prepare(0).expect("prepare first");
    observe_file(&mut session, &paths[0]);
    assert!(session.finish_track(0, true).expect("first end").is_none());
    session.prepare(1).expect("prepare second");
    observe_file(&mut session, &paths[1]);
    assert!(
        session
            .finish_track(1, false)
            .expect("interruption")
            .is_none()
    );
    assert!(!session.is_measuring());
    assert!(!conclusions::conclusions_path(&directory).exists());
    let mut next =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("next session");
    assert!(next.prepare(0).expect("next prepare").is_none());
    assert!(next.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn cached_silence_does_not_restart_measurement_on_later_plays() {
    let directory = root();
    let path = fixture_from(&directory, "silence.flac", "audit-silence.flac");
    let paths = vec![path.clone()];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    assert!(session.prepare(0).expect("prepare").is_none());
    assert!(session.is_measuring());
    observe_file(&mut session, &path);
    session
        .finish_track(0, true)
        .expect("finish")
        .expect("update")
        .save(&directory)
        .expect("save silence result");
    let cache = conclusions::load(&conclusions::conclusions_path(&directory))
        .expect("cache load")
        .expect("cache exists");
    assert_eq!(cache.loudness_tracks.len(), 1);
    assert!(
        cache
            .loudness_tracks
            .values()
            .next()
            .expect("cached track")
            .measurement
            .is_none()
    );
    for _ in 0..2 {
        let mut next =
            ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
                .expect("next session");
        assert!(next.prepare(0).expect("cached silence").is_none());
        assert!(!next.is_measuring());
        assert!(next.finish_track(0, true).expect("finish").is_none());
    }
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn an_unknown_album_can_restart_capture_without_changing_its_frozen_gain() {
    let directory = root();
    let paths = vec![
        fixture(&directory, "first.flac"),
        fixture_from(&directory, "second.flac", "playback-dualmono.flac"),
    ];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("session");
    session.prepare(0).expect("prepare first");
    observe_file(&mut session, &paths[0]);
    assert!(session.finish_track(0, true).expect("first end").is_none());
    session.prepare(1).expect("prepare second");
    observe_file(&mut session, &paths[1]);
    assert!(
        session
            .finish_track(1, false)
            .expect("skip second")
            .is_none()
    );
    assert!(!session.is_measuring());

    assert!(session.prepare(0).expect("restart first").is_none());
    assert!(session.is_measuring());
    observe_file(&mut session, &paths[0]);
    assert!(session.finish_track(0, true).expect("first end").is_none());
    assert!(
        session
            .prepare(1)
            .expect("second gain stays fixed")
            .is_none()
    );
    observe_file(&mut session, &paths[1]);
    session
        .finish_track(1, true)
        .expect("album end")
        .expect("update")
        .save(&directory)
        .expect("save");
    let cache = conclusions::load(&conclusions::conclusions_path(&directory))
        .expect("load")
        .expect("cache");
    assert_eq!(cache.loudness_programmes.len(), 1);
    assert_eq!(cache.loudness_programmes[0].files.len(), 2);
    assert_eq!(
        cache.loudness_programmes[0].measurement,
        loudness::measure_programme(&[paths[0].as_path(), paths[1].as_path()]).expect("reference"),
    );
    assert!(
        session
            .prepare(0)
            .expect("another play stays fixed")
            .is_none()
    );
    assert!(!session.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn unsupported_track_loudness_is_cached_after_complete_valid_decoding() {
    let directory = root();
    let path = surround_fixture(&directory);
    let paths = vec![path.clone()];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    session.prepare(0).expect("prepare");
    observe_unsupported_file(&mut session, &path);
    session
        .finish_track(0, true)
        .expect("finish")
        .expect("update")
        .save(&directory)
        .expect("save unavailable result");
    let cache = conclusions::load(&conclusions::conclusions_path(&directory))
        .expect("load")
        .expect("cache");
    assert_eq!(cache.loudness_tracks.len(), 1);
    assert!(
        cache
            .loudness_tracks
            .values()
            .next()
            .expect("track")
            .measurement
            .is_none()
    );
    let mut next =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("next session");
    assert!(next.prepare(0).expect("next gain").is_none());
    assert!(!next.is_measuring());
    assert!(next.finish_track(0, true).expect("next finish").is_none());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn one_unsupported_album_section_makes_the_whole_programme_unavailable() {
    let directory = root();
    let paths = vec![
        fixture(&directory, "first.flac"),
        surround_fixture(&directory),
    ];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("session");
    session.prepare(0).expect("prepare first");
    observe_file(&mut session, &paths[0]);
    assert!(session.finish_track(0, true).expect("first end").is_none());
    session.prepare(1).expect("prepare second");
    observe_unsupported_file(&mut session, &paths[1]);
    session
        .finish_track(1, true)
        .expect("album end")
        .expect("update")
        .save(&directory)
        .expect("save unavailable programme");
    let cache = conclusions::load(&conclusions::conclusions_path(&directory))
        .expect("load")
        .expect("cache");
    assert_eq!(cache.loudness_programmes.len(), 1);
    assert_eq!(cache.loudness_programmes[0].files.len(), 2);
    assert!(cache.loudness_programmes[0].measurement.is_none());
    let mut next =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("next session");
    for index in 0..paths.len() {
        assert!(next.prepare(index).expect("cached gain").is_none());
        assert!(!next.is_measuring());
    }
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn unsupported_capture_still_rejects_fragments_and_changed_sources() {
    let directory = root();
    let path = surround_fixture(&directory);
    let paths = vec![path.clone()];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    session.prepare(0).expect("prepare");
    observe_unsupported_file(&mut session, &path);
    assert!(session.finish_track(0, false).expect("skip").is_none());
    session.prepare(0).expect("restart");
    observe_unsupported_file(&mut session, &path);
    append_source_byte(&path);
    assert!(session.finish_track(0, true).is_err());
    assert!(!conclusions::conclusions_path(&directory).exists());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn invalid_source_pcm_abandons_capture_instead_of_caching_unavailability() {
    let directory = root();
    let path = fixture(&directory, "first.flac");
    let paths = vec![path];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Track)
            .expect("session");
    session.prepare(0).expect("prepare");
    let format = PcmFormat::new(44_100, 2).expect("format");
    assert!(session.observe_source(format, &[f32::NAN, 0.0]).is_err());
    assert!(!session.is_measuring());
    assert!(session.finish_track(0, true).expect("finish").is_none());
    assert!(!conclusions::conclusions_path(&directory).exists());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn entering_an_unknown_album_midway_still_allows_learning_from_its_start() {
    let directory = root();
    let paths = vec![
        fixture(&directory, "first.flac"),
        fixture(&directory, "second.flac"),
    ];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("session");
    assert!(session.prepare(1).expect("middle gain").is_none());
    assert!(!session.is_measuring());
    assert!(session.finish_track(1, true).expect("middle end").is_none());
    assert!(session.prepare(0).expect("beginning gain").is_none());
    assert!(session.is_measuring());
    for (index, path) in paths.iter().enumerate() {
        assert!(session.prepare(index).expect("fixed gain").is_none());
        observe_file(&mut session, path);
        let update = session.finish_track(index, true).expect("complete track");
        assert_eq!(update.is_some(), index + 1 == paths.len());
    }
    assert!(session.prepare(0).expect("later fixed gain").is_none());
    assert!(!session.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}

#[test]
fn an_interrupted_unsupported_album_does_not_publish_unavailability() {
    let directory = root();
    let paths = vec![
        surround_fixture(&directory),
        fixture(&directory, "second.flac"),
    ];
    let mut session =
        ReadyNormalization::new(&paths, false, None, &directory, NormalizationMode::Album)
            .expect("session");
    session.prepare(0).expect("prepare first");
    observe_unsupported_file(&mut session, &paths[0]);
    assert!(session.finish_track(0, true).expect("first end").is_none());
    session.prepare(1).expect("prepare second");
    observe_file(&mut session, &paths[1]);
    assert!(
        session
            .finish_track(1, false)
            .expect("interruption")
            .is_none()
    );
    assert!(!conclusions::conclusions_path(&directory).exists());
    session.prepare(0).expect("restart");
    assert!(session.is_measuring());
    std::fs::remove_dir_all(directory).expect("cleanup");
}
