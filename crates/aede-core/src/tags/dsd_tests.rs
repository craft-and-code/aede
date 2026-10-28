use super::super::test_support::BinaryFile;
#[test]
fn scans_source_clock_and_end_id3_without_claiming_pcm_precision() {
    let mut bytes = include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf").to_vec();
    let offset = bytes.len() as u64;
    let mut body = b"TIT2\0\0\0\x06\0\0\x03Tone!".to_vec();
    let mut id3 = b"ID3\x03\0\0\0\0\0\x10".to_vec();
    id3.append(&mut body);
    bytes[20..28].copy_from_slice(&offset.to_le_bytes());
    bytes.extend(id3);
    let length = bytes.len() as u64;
    bytes[12..20].copy_from_slice(&length.to_le_bytes());
    let temp = BinaryFile::new("dsf", &bytes);
    let tags = temp.read().unwrap();
    assert_eq!(tags.first("title"), Some("Tone!"));
    assert_eq!(tags.properties.sample_rate, Some(2_822_400));
    assert_eq!(tags.properties.bit_depth, Some(1));
    assert_eq!(tags.properties.quality_label(), "DSD64");
    assert!(super::super::is_audio_path(std::path::Path::new(
        "album/track.DSF"
    )));
}
#[test]
fn truncated_dsd_does_not_fall_back_to_a_different_parser() {
    let temp = BinaryFile::new("dsf", b"DSD bad header bytes");
    assert!(super::super::read(temp.path()).is_err());
}

#[test]
fn appended_id3_cannot_read_beyond_the_dsd_metadata_extent() {
    let mut bytes = include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf").to_vec();
    let offset = bytes.len() as u64;
    bytes[20..28].copy_from_slice(&offset.to_le_bytes());
    bytes.extend(b"ID3\x03\0\0\0\0\x01\0");
    let len = bytes.len() as u64;
    bytes[12..20].copy_from_slice(&len.to_le_bytes());
    assert!(BinaryFile::new("dsf", &bytes).read().is_err());
}

#[test]
fn catalogue_retains_valid_higher_dsd_clocks_without_admitting_pcm() {
    for (extension, mut bytes) in [
        (
            "dsf",
            include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf").to_vec(),
        ),
        (
            "dff",
            include_bytes!("../../../aede-audio/tests/fixtures/tone.dff").to_vec(),
        ),
    ] {
        if extension == "dsf" {
            bytes[56..60].copy_from_slice(&11_289_600u32.to_le_bytes());
        } else {
            let at = bytes.windows(4).position(|w| w == b"FS  ").unwrap() + 12;
            bytes[at..at + 4].copy_from_slice(&11_289_600u32.to_be_bytes());
        }
        let file = BinaryFile::new(extension, &bytes);
        let tags = file.read().unwrap();
        assert_eq!(tags.properties.sample_rate, Some(11_289_600));
        assert_eq!(tags.properties.bit_depth, Some(1));
        assert_eq!(tags.properties.quality_label(), "DSD256");
        assert!(
            aede_audio::dsd::Decoder::open(file.path(), aede_audio::dsd::Profile::Listening)
                .is_err()
        );
    }
}
#[test]
fn catalogue_retains_valid_dsf_multichannel_without_claiming_native_playback() {
    let original = include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf");
    let mut bytes = original[..92].to_vec();
    for _ in 0..6 {
        bytes.extend_from_slice(&original[92..]);
    }
    bytes[48..52].copy_from_slice(&7u32.to_le_bytes());
    bytes[52..56].copy_from_slice(&6u32.to_le_bytes());
    let length = bytes.len() as u64;
    bytes[12..20].copy_from_slice(&length.to_le_bytes());
    bytes[84..92].copy_from_slice(&(length - 80).to_le_bytes());
    let file = BinaryFile::new("dsf", &bytes);
    let tags = file.read().unwrap();
    assert_eq!(tags.properties.channels, Some(6));
    assert!(
        aede_audio::dsd::Decoder::open(file.path(), aede_audio::dsd::Profile::Listening).is_err()
    );
}
#[test]
fn unknown_clock_families_never_receive_a_truncated_dsd_level() {
    let properties = super::super::AudioProperties {
        codec: "dsd".into(),
        sample_rate: Some(3_072_000),
        ..Default::default()
    };
    assert_eq!(properties.quality_label(), "DSD");
}

#[test]
fn unsupported_source_clocks_cannot_overflow_catalogue_bitrate() {
    let original = include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf");
    let mut bytes = original[..92].to_vec();
    for _ in 0..6 {
        bytes.extend_from_slice(&original[92..]);
    }
    bytes[48..52].copy_from_slice(&7u32.to_le_bytes());
    bytes[52..56].copy_from_slice(&6u32.to_le_bytes());
    bytes[56..60].copy_from_slice(&u32::MAX.to_le_bytes());
    let length = bytes.len() as u64;
    bytes[12..20].copy_from_slice(&length.to_le_bytes());
    bytes[84..92].copy_from_slice(&(length - 80).to_le_bytes());
    let properties = BinaryFile::new("dsf", &bytes).read().unwrap().properties;
    assert_eq!(
        properties.bitrate_kbps,
        Some((u64::from(u32::MAX) * 6 / 1000) as u32)
    );
}

#[test]
fn advertised_dsd_rates_match_the_actual_shared_decoders() {
    for (name, bytes) in [
        (
            "tone.dsf",
            include_bytes!("../../../aede-audio/tests/fixtures/tone.dsf").as_slice(),
        ),
        (
            "tone.dff",
            include_bytes!("../../../aede-audio/tests/fixtures/tone.dff").as_slice(),
        ),
    ] {
        let file = BinaryFile::new(name.rsplit('.').next().unwrap(), bytes);
        let properties = file.read().unwrap().properties;
        let support = super::capabilities(&properties).unwrap();
        assert_eq!(support.native_pcm, Some(true));
        for (profile, advertised) in [
            (aede_audio::dsd::Profile::Listening, support.listening_rate),
            (
                aede_audio::dsd::Profile::Measurement,
                support.measurement_rate,
            ),
        ] {
            let decoder = aede_audio::dsd::Decoder::open(file.path(), profile).unwrap();
            assert_eq!(advertised, Some(decoder.sample_rate()));
        }
    }
}
