use std::path::PathBuf;

use super::{Error, FileDecoder};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
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
        ("track.ogg", 43_972),
    ] {
        if name == "track.ogg" && crate::ffmpeg::find().is_none() {
            continue;
        }
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
    let mut decoder = FileDecoder::open(&fixture("audit-stereo.flac")).expect("fixture opens");
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
    let (_, _, reference) = decode_all("audit-stereo.flac", 32);
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
