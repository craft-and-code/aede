use super::*;

#[test]
fn conversion_through_a_destination_hard_link_keeps_the_original_audio() {
    let Some(ffmpeg) = find_ffmpeg() else {
        assert!(std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none());
        return;
    };
    let dir = std::env::temp_dir().join(format!("aede_convert_hardlink_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let source = dir.join("source.flac");
    let target = dir.join("output.mp3");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac"),
        &source,
    )
    .unwrap();
    let original = std::fs::read(&source).unwrap();
    std::fs::hard_link(&source, &target).unwrap();
    let result = convert(&ffmpeg, &source, &target, Target::Mp3, None);
    assert!(
        std::fs::read(&source).unwrap() == original,
        "a destination alias must never truncate the source"
    );
    result.unwrap();
    assert_eq!(crate::tags::read(&target).unwrap().properties.codec, "mp3");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    assert!(convert(&ffmpeg, &source, &source, Target::Mp3, None).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn direct_conversion_refuses_a_linked_destination() {
    let Some(ffmpeg) = find_ffmpeg() else {
        assert!(std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none());
        return;
    };
    let dir = std::env::temp_dir().join(format!("aede_convert_symlink_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac");
    let victim = dir.join("victim.mp3");
    let target = dir.join("output.mp3");
    std::fs::write(&victim, b"keep original bytes").unwrap();
    std::os::unix::fs::symlink(&victim, &target).unwrap();
    assert!(convert(&ffmpeg, &source, &target, Target::Mp3, None).is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep original bytes");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_relative_protocol_looking_filename_is_encoded_as_a_local_file() {
    let Some(ffmpeg) = find_ffmpeg() else {
        assert!(std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none());
        return;
    };
    let source = std::path::PathBuf::from(format!("pipe:{}", std::process::id()));
    let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/track.flac");
    std::fs::copy(&original, &source).unwrap();
    let target =
        std::env::temp_dir().join(format!("aede_convert_local_{}.mp3", std::process::id()));
    let result = convert(&ffmpeg, &source, &target, Target::Mp3, None);
    std::fs::remove_file(source).unwrap();
    result.unwrap();
    assert_eq!(crate::tags::read(&target).unwrap().properties.codec, "mp3");
    std::fs::remove_file(target).unwrap();
}

#[test]
fn a_target_is_named_the_way_a_person_would_name_it() {
    assert_eq!(Target::parse("mp3"), Some(Target::Mp3));
    assert_eq!(Target::parse("MP3"), Some(Target::Mp3));
    // Two words for one thing, because a user asking for "aac" and one
    // asking for "m4a" mean the same file.
    assert_eq!(Target::parse("aac"), Some(Target::Aac));
    assert_eq!(Target::parse("m4a"), Some(Target::Aac));
    assert_eq!(Target::parse("ogg"), Some(Target::Vorbis));
    assert_eq!(Target::parse("wma"), None);
    assert_eq!(
        Target::Aac.extension(),
        "m4a",
        "the container, not the codec"
    );
}

#[test]
fn a_quality_that_parses_as_nothing_is_refused() {
    // An encoder silently running at a setting nobody asked for produces
    // files that are wrong in a way nobody notices until the card is full.
    assert_eq!(Quality::parse("V0"), Some(Quality::Variable(0)));
    assert_eq!(Quality::parse("v2"), Some(Quality::Variable(2)));
    assert_eq!(Quality::parse("q6"), Some(Quality::Variable(6)));
    assert_eq!(Quality::parse("192k"), Some(Quality::Bitrate(192)));
    assert_eq!(Quality::parse("320"), Some(Quality::Bitrate(320)));
    assert_eq!(Quality::parse("best"), None);
    assert_eq!(Quality::parse("V99"), None, "no such level");
    assert_eq!(Quality::parse("1k"), None, "not a bitrate anybody means");
}

#[test]
fn each_encoder_is_asked_in_its_own_terms() {
    // MP3 counts down and Vorbis counts up; flattening the two into one
    // number would give somebody asking for the best Vorbis the worst one.
    assert_eq!(
        quality_arguments(Target::Mp3, None),
        vec!["-q:a".to_string(), "0".to_string()]
    );
    assert_eq!(
        quality_arguments(Target::Vorbis, None),
        vec!["-q:a".to_string(), "6".to_string()]
    );
    assert_eq!(
        quality_arguments(Target::Opus, None),
        vec!["-b:a".to_string(), "128k".to_string()]
    );
    assert_eq!(
        quality_arguments(Target::Mp3, Some(Quality::Bitrate(320))),
        vec!["-b:a".to_string(), "320k".to_string()]
    );
    // A lossless target has no knob, so it is given none rather than one
    // that would be ignored.
    assert!(quality_arguments(Target::Flac, Some(Quality::Bitrate(320))).is_empty());
}

#[test]
fn only_mp3_aac_and_flac_can_carry_a_cover_across() {
    // The three containers ffmpeg can mux a picture stream into — and,
    // proven separately by running real ffmpeg against each target, the
    // exact three it does not refuse outright.
    assert!(Target::Mp3.keeps_embedded_art());
    assert!(Target::Aac.keeps_embedded_art());
    assert!(Target::Flac.keeps_embedded_art());
    assert!(!Target::Wav.keeps_embedded_art());
    assert!(!Target::Opus.keeps_embedded_art());
    assert!(!Target::Vorbis.keeps_embedded_art());
}

#[test]
fn wav_drops_what_its_legacy_info_chunk_has_no_room_for() {
    let mut present = BTreeMap::new();
    for key in ["title", "artist", "album", "genre", "date", "tracknumber"] {
        present.insert(key.to_string(), vec!["x".to_string()]);
    }
    assert!(
        Target::Wav.tags_it_would_drop(&present).is_empty(),
        "the six wav is known to keep"
    );

    present.insert("composer".to_string(), vec!["x".to_string()]);
    present.insert("albumartist".to_string(), vec!["x".to_string()]);
    let mut dropped = Target::Wav.tags_it_would_drop(&present);
    dropped.sort();
    assert_eq!(
        dropped,
        vec!["albumartist".to_string(), "composer".to_string()]
    );

    // Every other target's tag format takes an arbitrary key: nothing
    // named here because nothing here is known to be lost.
    assert!(Target::Mp3.tags_it_would_drop(&present).is_empty());
    assert!(Target::Opus.tags_it_would_drop(&present).is_empty());
    assert!(Target::Vorbis.tags_it_would_drop(&present).is_empty());
}

#[test]
fn a_size_is_estimated_from_the_playing_time() {
    // Four minutes at 128 kbps is about 3.8 MB, and the point of the figure
    // is to answer "will this fit" before an hour of encoding.
    let four_minutes = 240_000;
    let estimate = estimated_size(Target::Opus, None, four_minutes, 40_000_000);
    assert!((3_500_000..4_200_000).contains(&estimate), "got {estimate}");
    // And a bitrate that was asked for is the one used.
    let higher = estimated_size(Target::Opus, Some(Quality::Bitrate(256)), four_minutes, 0);
    assert!(higher > estimate * 3 / 2, "{higher} vs {estimate}");
}

#[test]
fn quality_scales_and_bitrates_are_specific_to_the_encoder() {
    for (target, word) in [
        (Target::Mp3, "V10"),
        (Target::Mp3, "q6"),
        (Target::Opus, "V0"),
        (Target::Opus, "512k"),
        (Target::Aac, "q10"),
        (Target::Vorbis, "V0"),
        (Target::Mp3, "191k"),
        (Target::Flac, "128k"),
    ] {
        assert_eq!(Quality::parse_for(target, word), None, "{target:?}: {word}");
    }
    assert_eq!(
        Quality::parse_for(Target::Mp3, "V9"),
        Some(Quality::Variable(9))
    );
    assert_eq!(
        Quality::parse_for(Target::Vorbis, "q10"),
        Some(Quality::Variable(10))
    );
    assert_eq!(
        Quality::parse_for(Target::Opus, "128k"),
        Some(Quality::Bitrate(128))
    );
    assert!(
        estimated_size(Target::Mp3, Some(Quality::Variable(9)), 60_000, 0)
            < estimated_size(Target::Mp3, Some(Quality::Variable(0)), 60_000, 0)
    );
}

#[test]
fn wav_conversion_preserves_real_twenty_four_bit_samples() {
    let Some(ffmpeg) = find_ffmpeg() else {
        assert!(
            std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none(),
            "WAV conversion regression requires ffmpeg"
        );
        eprintln!("WAV conversion regression needs ffmpeg; conversion not exercised");
        return;
    };
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audit-real24.flac");
    let dir = std::env::temp_dir().join(format!("aede_copy_wav24_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let destination = dir.join("output.wav");
    convert(&ffmpeg, &source, &destination, Target::Wav, None).unwrap();
    let source_properties = crate::tags::read(&source).unwrap().properties;
    let properties = crate::tags::read(&destination).unwrap().properties;
    assert_eq!(source_properties.bit_depth, Some(24));
    assert_eq!(properties.bit_depth, source_properties.bit_depth);
    assert_eq!(properties.sample_rate, source_properties.sample_rate);
    // Compare actual PCM, including low bits, rather than trusting a 24-bit header.
    let decode = |path: &Path| {
        let output = Command::new(&ffmpeg)
            .args(["-nostdin", "-loglevel", "error", "-i"])
            .arg(path)
            .args(["-map", "0:a", "-f", "s32le", "-c:a", "pcm_s32le", "-"])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    assert_eq!(decode(&source), decode(&destination));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn floating_pcm_conversion_preserves_over_fullscale_and_tiny_samples() {
    let Some(ffmpeg) = find_ffmpeg() else {
        assert!(
            std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none(),
            "floating PCM conversion regression requires ffmpeg"
        );
        eprintln!("floating PCM conversion regression needs ffmpeg");
        return;
    };
    let dir = std::env::temp_dir().join(format!("aede_copy_wav_float_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let raw = dir.join("input.f32");
    let samples: Vec<u8> = [1.5f32, 0.1, 1e-10, -1.5]
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    std::fs::write(&raw, &samples).unwrap();
    let source = dir.join("source.aif");
    let output = Command::new(&ffmpeg)
        .args([
            "-nostdin",
            "-loglevel",
            "error",
            "-f",
            "f32le",
            "-ar",
            "44100",
            "-ac",
            "1",
            "-i",
        ])
        .arg(&raw)
        .args(["-c:a", "pcm_f32be"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let destination = dir.join("output.wav");
    convert(&ffmpeg, &source, &destination, Target::Wav, None).unwrap();
    let properties = crate::tags::read(&destination).unwrap().properties;
    assert_eq!(properties.codec, "pcm_float");
    assert_eq!(properties.bit_depth, Some(32));
    let decoded = Command::new(&ffmpeg)
        .args(["-nostdin", "-loglevel", "error", "-i"])
        .arg(&destination)
        .args(["-map", "0:a", "-f", "f32le", "-c:a", "pcm_f32le", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, samples);
    assert!(crate::tags::read(&source).unwrap().properties.lossless);
    let flac = dir.join("output.flac");
    assert!(
        convert(&ffmpeg, &source, &flac, Target::Flac, None).is_err(),
        "FLAC cannot preserve arbitrary floating PCM"
    );
    assert!(
        !flac.exists(),
        "an unsupported lossless conversion must not write output"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn integer_thirty_two_bit_flac_conversion_preserves_low_bits() {
    let Some(ffmpeg) = find_ffmpeg() else {
        assert!(
            std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none(),
            "32-bit FLAC conversion regression requires ffmpeg"
        );
        eprintln!("32-bit FLAC conversion regression needs ffmpeg");
        return;
    };
    let dir = std::env::temp_dir().join(format!("aede_copy_flac32_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let raw = dir.join("input.s32");
    let samples: Vec<u8> = [i32::MAX, 123456789, 1, -2]
        .iter()
        .cycle()
        .take(400)
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    std::fs::write(&raw, &samples).unwrap();
    let source = dir.join("source.wav");
    let output = Command::new(&ffmpeg)
        .args([
            "-nostdin",
            "-loglevel",
            "error",
            "-f",
            "s32le",
            "-ar",
            "44100",
            "-ac",
            "1",
            "-i",
        ])
        .arg(&raw)
        .args(["-c:a", "pcm_s32le"])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let destination = dir.join("output.flac");
    let source_before = std::fs::read(&source).unwrap();
    let temporary = super::super::TemporaryOutput::new(&destination).unwrap();
    if let Err(reason) = convert(&ffmpeg, &source, temporary.path(), Target::Flac, None) {
        assert!(
            reason.contains("ffmpeg") || reason.contains("precision"),
            "{reason}"
        );
        drop(temporary);
        assert!(
            !destination.exists(),
            "an incapable encoder must not publish output"
        );
        assert_eq!(std::fs::read(&source).unwrap(), source_before);
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            2,
            "temporary output was removed"
        );
        std::fs::remove_dir_all(dir).unwrap();
        return;
    }
    temporary.publish().unwrap();
    assert_eq!(
        crate::tags::read(&destination)
            .unwrap()
            .properties
            .bit_depth,
        Some(32)
    );
    let decoded = Command::new(&ffmpeg)
        .args(["-nostdin", "-loglevel", "error", "-i"])
        .arg(&destination)
        .args(["-map", "0:a", "-f", "s32le", "-c:a", "pcm_s32le", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success());
    assert_eq!(decoded.stdout, samples);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_lossless_encoder_reporting_lower_precision_is_refused() {
    let source = crate::tags::AudioProperties {
        codec: "pcm".into(),
        bit_depth: Some(32),
        sample_rate: Some(44100),
        channels: Some(2),
        lossless: true,
        ..Default::default()
    };
    let mut written = source.clone();
    written.codec = "flac".into();
    written.bit_depth = Some(24);
    assert!(verify_lossless_properties(&source, &written).is_err());
    written.bit_depth = Some(32);
    assert!(verify_lossless_properties(&source, &written).is_ok());
    written.channels = Some(1);
    assert!(verify_lossless_properties(&source, &written).is_err());
}
