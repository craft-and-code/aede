use std::path::PathBuf;

use super::{Error, FileDecoder};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(if name.starts_with("playback") {
            "tests/playback_fixtures/flac"
        } else {
            "tests/fixtures"
        })
        .join(name)
}

#[test]
fn a_flac_md5_mismatch_is_not_reported_as_successful_decoded_eof() {
    let mut bytes = std::fs::read(fixture("playback-stereo.flac")).unwrap();
    assert_eq!(&bytes[..4], b"fLaC");
    assert!(bytes[26..42].iter().any(|&byte| byte != 0));
    bytes[26] ^= 1;
    let path = std::env::temp_dir().join(format!(
        "aede_flac_md5_mismatch_{}.flac",
        std::process::id()
    ));
    std::fs::write(&path, &bytes).unwrap();
    let result = (|| {
        let mut decoder = FileDecoder::open(&path).unwrap();
        let mut buffer = vec![0.0; 4096 * usize::from(decoder.channels())];
        loop {
            match decoder.read_frames(&mut buffer) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(error) => {
                    assert_eq!(
                        decoder.flac_md5_status(),
                        Some(super::FlacMd5Status::Mismatch)
                    );
                    assert!(matches!(
                        decoder.read_frames(&mut buffer),
                        Err(Error::FlacMd5Mismatch)
                    ));
                    return Some(error);
                }
            }
        }
    })();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.is_some_and(|error| error.to_string().contains("MD5")),
        "the decoded FLAC must fail when its stored audio digest differs"
    );
}

fn decode_all(name: &str, buffer_frames: usize) -> (u32, u16, Vec<f32>) {
    let mut decoder =
        FileDecoder::open(&fixture(name)).unwrap_or_else(|error| panic!("{name} opens: {error}"));
    let rate = decoder.sample_rate();
    let channels = decoder.channels();
    let mut output = vec![0.0; buffer_frames * usize::from(channels)];
    let mut samples = Vec::new();
    loop {
        let frames = decoder.read_frames(&mut output).expect("fixture decodes");
        if frames == 0 {
            break;
        }
        samples.extend_from_slice(&output[..frames * usize::from(channels)]);
    }
    assert_eq!(decoder.read_frames(&mut output).expect("stable EOF"), 0);
    (rate, channels, samples)
}

#[test]
fn supported_local_formats_yield_finite_complete_pcm_frames() {
    for name in ["track.flac", "track.wav", "track.mp3", "track.ogg"] {
        let (rate, channels, samples) = decode_all(name, 127);
        assert!(rate > 0, "{name}");
        assert!(channels > 0, "{name}");
        assert!(!samples.is_empty(), "{name}");
        assert_eq!(samples.len() % usize::from(channels), 0, "{name}");
        assert!(samples.iter().all(|sample| sample.is_finite()), "{name}");
    }
}

#[test]
fn multichannel_wav_preserves_its_declared_speaker_mask() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    let decoder = FileDecoder::open(&path).expect("fixture opens");
    assert_eq!(decoder.channels(), 6);
    assert_eq!(decoder.channel_layout().mask(), Some(0x3f));
    assert_eq!(decoder.channel_layout().name(), "5.1 (FL FR FC LFE BL BR)");
}

#[test]
fn small_reads_preserve_the_exact_decoded_stream() {
    let small = decode_all("hires.flac", 1);
    let large = decode_all("hires.flac", 4096);
    assert_eq!(small, large);
}

#[test]
fn encoder_delay_and_padding_do_not_reach_playback_pcm() {
    for (name, expected_frames) in [
        ("track.flac", 44_100),
        ("track.mp3", 44_100),
        ("gapless-stereo.mp3", 18_432),
        ("vbr.mp3", 44_100),
        ("track.ogg", 44_100),
    ] {
        let (_, channels, samples) = decode_all(name, 127);
        assert_eq!(
            samples.len() / usize::from(channels),
            expected_frames,
            "{name}"
        );
    }
}

#[test]
fn invalid_output_buffer_does_not_consume_audio() {
    let mut decoder = FileDecoder::open(&fixture("playback-stereo.flac")).expect("fixture opens");
    assert_eq!(decoder.channels(), 2);
    let channels = usize::from(decoder.channels());
    let mut empty = [];
    assert!(matches!(
        decoder.read_frames(&mut empty),
        Err(Error::InvalidBuffer)
    ));
    let mut incomplete = vec![42.0; channels + 1];
    assert!(matches!(
        decoder.read_frames(&mut incomplete),
        Err(Error::InvalidBuffer)
    ));
    assert!(incomplete.iter().all(|&sample| sample == 42.0));

    let mut output = vec![0.0; channels];
    assert_eq!(decoder.read_frames(&mut output).expect("first frame"), 1);
    let (_, _, reference) = decode_all("playback-stereo.flac", 32);
    assert_eq!(output, reference[..channels]);
}

#[test]
fn missing_file_is_an_error() {
    assert!(matches!(
        FileDecoder::open(&fixture("does-not-exist.flac")),
        Err(Error::Decode(_))
    ));
}

#[test]
fn ffmpeg_fallback_decodes_opus_and_m4a_without_changing_their_format() {
    if crate::ffmpeg::find().is_none() {
        eprintln!("skipped: ffmpeg is not installed");
        return;
    }
    for (name, rate, channels) in [
        ("track.opus", 48_000, 1),
        ("track.m4a", 44_100, 1),
        ("aac.m4a", 44_100, 1),
    ] {
        let (actual_rate, actual_channels, samples) = decode_all(name, 127);
        assert_eq!(actual_rate, rate, "{name}");
        assert_eq!(actual_channels, channels, "{name}");
        assert!(!samples.is_empty(), "{name}");
        assert!(samples.iter().all(|sample| sample.is_finite()), "{name}");
    }
}

#[test]
fn ffmpeg_fallback_preserves_samples_across_buffer_sizes() {
    if crate::ffmpeg::find().is_none() {
        eprintln!("skipped: ffmpeg is not installed");
        return;
    }
    assert_eq!(decode_all("track.opus", 1), decode_all("track.opus", 4096));
}

#[test]
fn opus_pre_skip_and_end_trim_yield_only_playable_frames() {
    if crate::ffmpeg::find().is_none() {
        eprintln!("skipped: ffmpeg is not installed");
        return;
    }
    let (rate, channels, samples) = decode_all("track.opus", 127);
    assert_eq!(rate, 48_000);
    assert_eq!(samples.len() / usize::from(channels), 48_000);
}

#[test]
fn progressive_skip_keeps_exact_native_pcm_and_clamps_at_actual_eof() {
    for name in [
        "playback-stereo.flac",
        "hires.flac",
        "track.wav",
        "track.mp3",
        "track.ogg",
    ] {
        let (_, channels, reference) = decode_all(name, 127);
        let channels = usize::from(channels);
        let source_frames = reference.len() / channels;
        for requested in [0, 1, 127, 4_099, source_frames as u64 + 1] {
            let mut decoder = FileDecoder::open(&fixture(name)).expect("fixture opens");
            let seek = decoder
                .skip_frames(requested, || false)
                .expect("seek succeeds");
            let actual = requested.min(source_frames as u64);
            assert_eq!(seek.frames, actual, "{name}, target {requested}");
            assert_eq!(seek.reached_eof, requested > source_frames as u64);
            let mut buffer = vec![0.0; 113 * channels];
            let mut suffix = Vec::new();
            loop {
                let frames = decoder.read_frames(&mut buffer).expect("suffix decodes");
                if frames == 0 {
                    break;
                }
                suffix.extend_from_slice(&buffer[..frames * channels]);
            }
            assert_eq!(suffix, reference[actual as usize * channels..], "{name}");
        }
    }
}

#[test]
fn progressive_skip_advances_from_a_partially_read_packet() {
    let (_, channels, reference) = decode_all("playback-stereo.flac", 127);
    let channels = usize::from(channels);
    let mut decoder = FileDecoder::open(&fixture("playback-stereo.flac")).expect("fixture opens");
    let mut frame = vec![0.0; channels];
    assert_eq!(decoder.read_frames(&mut frame).expect("first frame"), 1);
    assert_eq!(decoder.skip_frames(7, || false).unwrap().frames, 7);
    assert_eq!(
        decoder.read_frames(&mut frame).expect("frame after skip"),
        1
    );
    assert_eq!(frame, reference[8 * channels..9 * channels]);
}

#[test]
fn progressive_seek_cancellation_is_distinct_from_eof_and_polled_during_work() {
    let mut decoder = FileDecoder::open(&fixture("playback-stereo.flac")).expect("fixture opens");
    let mut polls = 0;
    let result = decoder.skip_frames(u64::MAX, || {
        polls += 1;
        polls == 3
    });
    assert!(matches!(result, Err(Error::SeekCancelled)));
    assert_eq!(polls, 3);

    let mut decoder = FileDecoder::open(&fixture("track.flac")).expect("fixture opens");
    assert!(matches!(
        decoder.skip_frames(0, || true),
        Err(Error::SeekCancelled)
    ));
}

#[test]
fn progressive_seek_preserves_ffmpeg_encoder_trim_and_source_samples() {
    if crate::ffmpeg::find().is_none() {
        eprintln!("skipped: ffmpeg is not installed");
        return;
    }
    for name in ["track.opus", "track.m4a", "aac.m4a"] {
        let (_, channels, reference) = decode_all(name, 127);
        let channels = usize::from(channels);
        let mut decoder = FileDecoder::open(&fixture(name)).expect("fixture opens");
        let skipped = 17_123;
        let seek = decoder
            .skip_frames(skipped, || false)
            .expect("fallback seeks");
        assert_eq!(seek.frames, skipped);
        assert!(!seek.reached_eof);
        let mut buffer = vec![0.0; 127 * channels];
        let mut suffix = Vec::new();
        loop {
            let frames = decoder.read_frames(&mut buffer).expect("fallback suffix");
            if frames == 0 {
                break;
            }
            suffix.extend_from_slice(&buffer[..frames * channels]);
        }
        assert_eq!(suffix, reference[skipped as usize * channels..], "{name}");
    }
}
