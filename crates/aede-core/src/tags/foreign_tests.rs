use super::*;

#[test]
fn wavpack_losslessness_follows_the_stream_in_path_and_descriptor_reads() {
    for (name, lossless, bit_depth) in [
        ("track.wv", true, Some(16)),
        ("hybrid-lossy.wv", false, None),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let mut file = std::fs::File::open(&path).unwrap();
        for tags in [
            super::super::read(&path).unwrap(),
            super::super::read_from_file(&mut file).unwrap(),
        ] {
            assert_eq!(tags.properties.codec, "wavpack", "{name}");
            assert_eq!(tags.properties.lossless, lossless, "{name}");
            assert_eq!(tags.properties.bit_depth, bit_depth, "{name}");
            assert_eq!(tags.properties.sample_rate, Some(44_100), "{name}");
            assert_eq!(tags.properties.channels, Some(2), "{name}");
        }
    }
}

#[test]
fn primary_ape_tags_precede_id3v1_and_retain_unique_legacy_values() {
    let mut bytes = include_bytes!("../../tests/fixtures/track.wv").to_vec();
    let mut legacy = [0u8; 128];
    legacy[..3].copy_from_slice(b"TAG");
    legacy[3..15].copy_from_slice(b"Legacy title");
    legacy[33..44].copy_from_slice(b"Miles Davis");
    legacy[63..75].copy_from_slice(b"Legacy album");
    legacy[93..97].copy_from_slice(b"1959");
    legacy[97..116].copy_from_slice(b"Legacy-only comment");
    legacy[127] = 255;
    bytes.extend_from_slice(&legacy);
    let fixture = super::super::test_support::BinaryFile::new("wv", &bytes);
    let tags = fixture.read().unwrap();

    assert_eq!(tags.all("title"), ["Blue in Green", "Legacy title"]);
    assert_eq!(tags.all("album"), ["Kind of Blue", "Legacy album"]);
    assert_eq!(tags.all("artist"), ["Miles Davis"]);
    assert_eq!(tags.all("date"), ["1959"]);
    assert_eq!(tags.first("comment"), Some("Legacy-only comment"));
    assert_eq!(tags.first("genre"), Some("Jazz"));
    assert_eq!(tags.first("tracknumber"), Some("3"));
}

#[test]
fn floating_wavpack_cannot_be_silently_quantized_by_lossless_conversion() {
    use crate::copy::transcode::{self, Target};

    let original = include_bytes!("../../tests/fixtures/float32.wv");
    let mut metadata = original[..32].to_vec();
    metadata[4..8].copy_from_slice(&24u32.to_le_bytes());
    metadata[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    metadata[16..32].fill(0);
    let ffmpeg = transcode::find_ffmpeg();
    assert!(
        ffmpeg.is_some() || std::env::var_os("AEDE_REQUIRE_FFMPEG").is_none(),
        "floating WavPack conversion regression requires ffmpeg"
    );
    for prefix in [Vec::new(), metadata.clone(), original[512..].to_vec()] {
        let mut bytes = prefix;
        bytes.extend_from_slice(original);
        let source = super::super::test_support::BinaryFile::new("wv", &bytes);
        let reads = [source.read().unwrap(), source.read_from_file().unwrap()];
        if let Some(ffmpeg) = &ffmpeg {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("aede_float_wavpack_{}_{nonce}", std::process::id()));
            std::fs::create_dir(&directory).unwrap();
            for target in [Target::Wav, Target::Flac] {
                let destination = directory.join(format!("output.{}", target.extension()));
                std::fs::write(&destination, b"existing destination").unwrap();
                let result = transcode::convert(ffmpeg, source.path(), &destination, target, None);
                assert!(
                    result
                        .as_ref()
                        .is_err_and(|reason| reason.contains("precision")),
                    "{target:?} must refuse before quantizing the source: {result:?}"
                );
                assert_eq!(
                    std::fs::read(&destination).unwrap(),
                    b"existing destination"
                );
            }
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
            std::fs::remove_dir_all(directory).unwrap();
        }
        for tags in reads {
            assert_eq!(tags.properties.codec, "wavpack");
            assert!(tags.properties.lossless);
            assert_eq!(tags.properties.bit_depth, None);
            assert_eq!(tags.properties.sample_rate, Some(8_000));
            assert_eq!(tags.properties.channels, Some(1));
            for target in [Target::Wav, Target::Flac] {
                assert!(transcode::validate_conversion(&tags.properties, target).is_err());
            }
            assert!(transcode::validate_conversion(&tags.properties, Target::Mp3).is_ok());
        }
        assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
    }
    for (blocks, depth) in [(1, Some(16)), (65, None)] {
        let mut bytes = metadata.repeat(blocks);
        bytes.extend_from_slice(include_bytes!("../../tests/fixtures/track.wv"));
        let source = super::super::test_support::BinaryFile::new("wv", &bytes);
        for tags in [source.read().unwrap(), source.read_from_file().unwrap()] {
            assert!(tags.properties.lossless);
            assert_eq!(tags.properties.bit_depth, depth);
        }
    }
}
