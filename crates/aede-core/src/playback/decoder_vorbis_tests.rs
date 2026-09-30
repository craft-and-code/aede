use std::path::{Path, PathBuf};

use super::FileDecoder;

#[path = "decoder_vorbis_fixtures.rs"]
mod fixtures;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/playback_fixtures")
        .join(name)
}

fn decode(path: &Path, buffer_frames: usize) -> Vec<f32> {
    let mut decoder = FileDecoder::open(path).expect("Vorbis fixture opens");
    let channels = usize::from(decoder.channels());
    let mut buffer = vec![0.0; buffer_frames * channels];
    let mut samples = Vec::new();
    loop {
        let frames = decoder.read_frames(&mut buffer).expect("Vorbis decodes");
        if frames == 0 {
            break;
        }
        samples.extend_from_slice(&buffer[..frames * channels]);
    }
    assert_eq!(decoder.read_frames(&mut buffer).expect("stable EOF"), 0);
    samples
}

#[test]
fn vorbis_single_page_and_multiple_page_streams_keep_exact_playable_frames() {
    for (name, channels, expected_frames) in [
        ("vorbis-mono-101.ogg", 1, 101),
        ("vorbis-stereo-3200.ogg", 2, 3_200),
        ("vorbis-stereo-10001.ogg", 2, 10_001),
        ("vorbis-stereo-300013.ogg", 2, 300_013),
    ] {
        let path = fixture(name);
        let decoder = FileDecoder::open(&path).expect("fixture format");
        assert_eq!(decoder.sample_rate(), 48_000);
        assert_eq!(usize::from(decoder.channels()), channels);
        let samples = decode(&path, 127);
        assert_eq!(samples.len(), expected_frames * channels, "{name}");
        assert!(samples.iter().all(|sample| sample.is_finite()), "{name}");
    }
}

#[test]
fn vorbis_end_trimming_is_independent_of_caller_buffer_size() {
    for name in ["vorbis-mono-101.ogg", "vorbis-stereo-10001.ogg"] {
        let path = fixture(name);
        assert_eq!(decode(&path, 1), decode(&path, 4_096), "{name}");
    }
}

#[test]
fn vorbis_start_and_end_samples_match_the_independent_xiph_reference() {
    let reference = crate::json::parse(include_str!(
        "../../tests/playback_fixtures/vorbis-reference.json"
    ))
    .expect("checked-in Xiph reference");
    for case in reference
        .get("fixtures")
        .and_then(crate::json::Json::as_arr)
        .expect("reference cases")
    {
        let name = case
            .get("filename")
            .and_then(crate::json::Json::as_str)
            .expect("fixture filename");
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(case.field_str("directory").expect("fixture directory"))
            .join(name);
        assert_xiph_reference(&path, name, case);
    }
    let cropped = fixtures::TempVorbis::new(&fixtures::repaged(-100));
    let case = reference
        .get("derived_cases")
        .and_then(crate::json::Json::as_arr)
        .expect("derived reference cases")
        .first()
        .expect("cropped reference");
    assert_xiph_reference(cropped.path(), "cropped-start-100", case);
}

fn assert_xiph_reference(path: &Path, name: &str, case: &crate::json::Json) {
    let decoder = FileDecoder::open(path).expect("reference fixture format");
    assert_eq!(
        u64::from(decoder.sample_rate()),
        case.field_u64("sample_rate").expect("rate")
    );
    let channels = decoder.channels();
    assert_eq!(
        u64::from(channels),
        case.field_u64("channels").expect("channels")
    );
    let actual = decode(path, 127);
    assert_eq!(
        actual.len() / usize::from(channels),
        case.field_u64("frames").expect("frames") as usize
    );
    for (key, tail) in [("first_16_frames", false), ("last_16_frames", true)] {
        let expected = case
            .get(key)
            .and_then(crate::json::Json::as_arr)
            .expect("reference samples");
        assert_eq!(expected.len(), usize::from(channels) * 16);
        let offset = if tail {
            actual.len() - expected.len()
        } else {
            0
        };
        for (index, expected) in expected.iter().enumerate() {
            let expected = expected.as_f64().expect("finite reference PCM");
            let error = (f64::from(actual[offset + index]) - expected).abs();
            assert!(error <= 2e-6, "{name} {key} sample {index}: error {error}");
        }
    }
}

#[test]
fn vorbis_prefix_crop_discards_the_beginning_instead_of_the_ending() {
    let normal = fixtures::TempVorbis::new(&fixtures::repaged(0));
    let cropped = fixtures::TempVorbis::new(&fixtures::repaged(-100));
    let reference = decode(normal.path(), 127);
    let actual = decode(cropped.path(), 1);
    assert_eq!(reference.len(), 44_100);
    assert_eq!(actual, reference[100..]);
}

#[test]
fn vorbis_positive_granule_origin_does_not_add_silence_or_remove_samples() {
    let normal = fixtures::TempVorbis::new(&fixtures::repaged(0));
    let shifted = fixtures::TempVorbis::new(&fixtures::repaged(10_000));
    assert_eq!(decode(normal.path(), 4096), decode(shifted.path(), 127));
}

#[test]
fn vorbis_without_a_known_final_granule_is_an_error() {
    for (granule, eos) in [(44_100, false), (u64::MAX, true), (0, true)] {
        let file = fixtures::TempVorbis::new(&fixtures::final_page(granule, eos));
        assert!(
            FileDecoder::open(file.path()).is_err(),
            "granule={granule}, eos={eos}"
        );
    }
}

#[test]
fn vorbis_does_not_turn_a_second_chained_stream_into_successful_eof() {
    let source = fixtures::original();
    let file = fixtures::TempVorbis::new(&[source.as_slice(), source.as_slice()].concat());
    assert_decode_error(file.path());
}

#[test]
fn vorbis_does_not_silently_discard_more_than_the_last_packets_tail() {
    let file = fixtures::TempVorbis::new(&fixtures::final_page(1_000, true));
    assert_decode_error(file.path());
}

#[test]
fn vorbis_rejects_a_truncated_initial_audio_page_or_a_corrupt_header_checksum() {
    let mut source = fixtures::original();
    source.truncate(source.len() - 25);
    let truncated = fixtures::TempVorbis::new(&source);
    assert!(FileDecoder::open(truncated.path()).is_err());
    let mut source = fixtures::original();
    source[22] ^= 1;
    let corrupt = fixtures::TempVorbis::new(&source);
    assert!(FileDecoder::open(corrupt.path()).is_err());
}

fn assert_decode_error(path: &Path) {
    let mut decoder = FileDecoder::open(path).expect("header has a known granule");
    let mut buffer = vec![0.0; usize::from(decoder.channels()) * 127];
    loop {
        match decoder.read_frames(&mut buffer) {
            Err(_) => break,
            Ok(0) => panic!("invalid Vorbis stream must not become successful EOF"),
            Ok(_) => {}
        }
    }
}
