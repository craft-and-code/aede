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
fn shared_native_opus_plays_and_seeks_without_an_external_decoder() {
    for (name, channels, expected_frames) in
        [("opus-mono.opus", 1, 12037), ("opus-stereo.opus", 2, 48013)]
    {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures")
            .join(name);
        let mut decoder = FileDecoder::open_native(&path).unwrap();
        assert_eq!(decoder.sample_rate(), 48000);
        assert_eq!(decoder.channels(), channels);
        let mut reference = Vec::new();
        let mut buffer = vec![0.0; 1024 * usize::from(channels)];
        loop {
            let frames = decoder.read_frames(&mut buffer).unwrap();
            if frames == 0 {
                break;
            }
            reference.extend_from_slice(&buffer[..frames * usize::from(channels)]);
        }
        assert_eq!(reference.len(), expected_frames * usize::from(channels));
        let mut seek = FileDecoder::open_native(&path).unwrap();
        let skipped = seek.skip_frames(4099, || false).unwrap();
        assert_eq!(skipped.frames, 4099);
        let frames = seek.read_frames(&mut buffer).unwrap();
        assert_eq!(
            &buffer[..frames * usize::from(channels)],
            &reference[4099 * usize::from(channels)..(4099 + frames) * usize::from(channels)]
        );
        let mut cancelled = FileDecoder::open_native(&path).unwrap();
        assert!(cancelled.skip_frames(4099, || true).is_err());
    }
}

#[test]
fn unsupported_multistream_opus_keeps_its_explicit_playback_compatibility() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-audio/tests/fixtures/opus-surround.opus");
    assert!(FileDecoder::open_native(&path).is_err());
    if crate::ffmpeg::find().is_some() {
        let mut decoder = FileDecoder::open(&path).unwrap();
        assert_eq!(decoder.channels(), 6);
        assert!(decoder.read_frames(&mut [0.0; 6 * 128]).unwrap() > 0);
    }
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

#[cfg(unix)]
#[test]
fn ffmpeg_fallback_treats_a_relative_protocol_like_filename_as_local_audio() {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    if crate::ffmpeg::find().is_none() {
        eprintln!("skipped: ffmpeg is not installed");
        return;
    }
    struct TemporaryInput(PathBuf);
    impl Drop for TemporaryInput {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    // A colon is a legal Unix filename character. FFmpeg must not strip its
    // "file:" spelling as a protocol and attempt a different local path.
    let input = TemporaryInput(PathBuf::from(format!(
        "file:aede_local_decoder_{}_{}_{nonce}.opus",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    )));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&input.0)
        .unwrap()
        .write_all(&std::fs::read(fixture("track.opus")).unwrap())
        .unwrap();
    let (_, channels, expected) = decode_all("track.opus", 127);
    let mut decoder = FileDecoder::open(&input.0).expect("local source opens");
    let mut buffer = vec![0.0; 127 * usize::from(channels)];
    let mut actual = Vec::new();
    loop {
        let frames = decoder
            .read_frames(&mut buffer)
            .expect("local filename is not a URL");
        if frames == 0 {
            break;
        }
        actual.extend_from_slice(&buffer[..frames * usize::from(channels)]);
    }
    assert_eq!(actual, expected);
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

#[test]
fn background_native_decoder_keeps_original_samples_and_complete_md5_checks() {
    let path = fixture("playback-stereo.flac");
    let mut background = FileDecoder::open_native(&path).unwrap();
    let mut ordinary = FileDecoder::open(&path).unwrap();
    let mut left = vec![0.; 4096 * 2];
    let mut right = vec![0.; 4096 * 2];
    loop {
        let a = background.read_frames(&mut left).unwrap();
        let b = ordinary.read_frames(&mut right).unwrap();
        assert_eq!(a, b);
        assert_eq!(left[..a], right[..b]);
        if a == 0 {
            break;
        }
    }
    assert_eq!(background.flac_md5_status(), ordinary.flac_md5_status());
}

#[path = "decoder_test_fixtures.rs"]
mod residual_fixtures;

type OpenDecoder = fn(&std::path::Path) -> Result<FileDecoder, Error>;

fn decode_with_layout_preserving_source(
    path: &std::path::Path,
    open: OpenDecoder,
) -> (u32, u16, aede_dsp::ChannelLayout, Vec<f32>) {
    let original = std::fs::read(path).unwrap();
    let decoded = (|| -> Result<_, Error> {
        let mut decoder = open(path)?;
        let format = (
            decoder.sample_rate(),
            decoder.channels(),
            decoder.channel_layout(),
        );
        let mut output = vec![0.0; 127 * usize::from(format.1)];
        let mut samples = Vec::new();
        loop {
            let frames = decoder.read_frames(&mut output)?;
            if frames == 0 {
                break;
            }
            samples.extend_from_slice(&output[..frames * usize::from(format.1)]);
        }
        assert_eq!(decoder.read_frames(&mut output)?, 0, "stable EOF");
        Ok((format.0, format.1, format.2, samples))
    })();
    assert_eq!(std::fs::read(path).unwrap(), original, "source bytes");
    let decoded = decoded.unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert!(!decoded.3.is_empty());
    assert!(decoded.3.iter().all(|sample| sample.is_finite()));
    decoded
}

fn assert_prefixed_multichannel_flac_preserves_audio(open: OpenDecoder) {
    let wave =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    let reference = decode_with_layout_preserving_source(&wave, open);
    assert_eq!((reference.0, reference.1), (48_000, 6));
    assert_eq!(reference.2.mask(), Some(0x3f));
    assert_eq!(reference.2.name(), "5.1 (FL FR FC LFE BL BR)");
    assert_eq!(reference.3.len(), 120 * 6);
    assert_eq!(
        decode_with_layout_preserving_source(&fixture("playback-5_1.flac"), open),
        reference,
        "the complete FLAC reference matches the existing WAV"
    );

    for optional_picture in [false, true] {
        let bytes = residual_fixtures::prefixed_surround_flac(optional_picture);
        let source = residual_fixtures::TemporarySource::new(&bytes);
        assert_eq!(
            decode_with_layout_preserving_source(source.path(), open),
            reference,
            "prefixed multichannel FLAC, optional picture: {optional_picture}"
        );
    }
}

#[test]
fn prefixed_multichannel_flac_preserves_layout_and_pcm_with_optional_metadata() {
    assert_prefixed_multichannel_flac_preserves_audio(FileDecoder::open);
}

#[test]
fn native_prefixed_multichannel_flac_preserves_layout_and_pcm_with_optional_metadata() {
    assert_prefixed_multichannel_flac_preserves_audio(FileDecoder::open_native);
}

#[test]
fn native_and_ordinary_decoders_preserve_existing_pcm_and_channel_layouts() {
    let surround =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    for (path, rate, channels, mask, frames) in [
        (surround, 48_000, 6, 0x3f, 120),
        (fixture("track.wav"), 44_100, 1, 0x1, 11_025),
        (fixture("track.mp3"), 44_100, 1, 0x1, 44_100),
        (fixture("gapless-stereo.mp3"), 44_100, 2, 0x3, 18_432),
        (fixture("playback-stereo.flac"), 44_100, 2, 0x3, 18_432),
    ] {
        let ordinary = decode_with_layout_preserving_source(&path, FileDecoder::open);
        let native = decode_with_layout_preserving_source(&path, FileDecoder::open_native);
        assert_eq!((ordinary.0, ordinary.1), (rate, channels));
        assert_eq!(ordinary.2.mask(), Some(mask));
        assert_eq!(ordinary.3.len(), frames * usize::from(channels));
        assert_eq!(native, ordinary, "{}", path.display());
    }
}

#[cfg(unix)]
fn assert_file_decoder_refuses_a_fifo(open: OpenDecoder, test_name: &str) {
    use std::os::unix::fs::FileTypeExt;

    const CHILD_PATH: &str = "AEDE_CORE_FLOAT_FIFO_TEST_PATH";
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        let path = PathBuf::from(path);
        assert!(matches!(open(&path), Err(Error::Decode(_))));
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_fifo()
        );
        std::fs::write(path.with_extension("completed"), b"refused").unwrap();
        return;
    }

    let source = residual_fixtures::TemporarySource::new(&[]);
    std::fs::remove_file(source.path()).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(source.path())
            .status()
            .unwrap()
            .success()
    );
    let worker = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name])
        .env(CHILD_PATH, source.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let status = residual_fixtures::wait_for_child(worker, std::time::Duration::from_secs(2));
    assert!(
        status.is_some(),
        "a FIFO needs no media writer to be refused"
    );
    assert!(status.unwrap().success(), "FIFO refusal child failed");
    assert_eq!(
        std::fs::read(source.path().with_extension("completed")).unwrap(),
        b"refused",
        "the exact child test reached the refusal assertion"
    );
    assert!(
        std::fs::symlink_metadata(source.path())
            .unwrap()
            .file_type()
            .is_fifo()
    );
}

#[cfg(unix)]
#[test]
fn file_admission_refuses_a_fifo_without_waiting_for_a_writer() {
    assert_file_decoder_refuses_a_fifo(
        FileDecoder::open,
        "playback::decoder::tests::file_admission_refuses_a_fifo_without_waiting_for_a_writer",
    );
}

#[cfg(unix)]
#[test]
fn native_file_admission_refuses_a_fifo_without_waiting_for_a_writer() {
    assert_file_decoder_refuses_a_fifo(
        FileDecoder::open_native,
        "playback::decoder::tests::native_file_admission_refuses_a_fifo_without_waiting_for_a_writer",
    );
}

fn surround_pcm_reference(open: OpenDecoder) -> Vec<f32> {
    let wave =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/channel_fixtures/surround-5_1.wav");
    let (rate, channels, layout, samples) = decode_with_layout_preserving_source(&wave, open);
    assert_eq!((rate, channels), (48_000, 6));
    assert_eq!(layout.mask(), Some(0x3f));
    assert_eq!(samples.len(), 120 * 6);
    samples
}

fn assert_verified_surround_format(decoder: &FileDecoder) {
    assert_eq!(decoder.sample_rate(), 48_000);
    assert_eq!(decoder.channels(), 6);
    assert_eq!(decoder.channel_layout().mask(), Some(0x3f));
    assert_eq!(decoder.channel_layout().name(), "5.1 (FL FR FC LFE BL BR)");
}

fn drain_surround_decoder(decoder: &mut FileDecoder) -> (Vec<f32>, Result<(), Error>) {
    let mut samples = Vec::new();
    let mut output = [0.0; 7 * 6];
    // Even one-frame reads must reach either EOF or an error on this 120-frame
    // fixture. A decoder that stops making progress cannot hang a regression.
    for _ in 0..=120 {
        match decoder.read_frames(&mut output) {
            Ok(0) => return (samples, Ok(())),
            Ok(frames) => samples.extend_from_slice(&output[..frames * 6]),
            Err(error) => return (samples, Err(error)),
        }
    }
    panic!("the short FLAC source did not terminate");
}

fn assert_prefixed_flac_verification_preserves_exact_pcm(open: OpenDecoder) {
    use super::FlacMd5Status;
    use residual_fixtures::FlacDigest;

    let reference = surround_pcm_reference(open);
    for leading_id3 in [false, true] {
        for optional_picture in [false, true] {
            for declared_frames in [120, 0] {
                for digest in [FlacDigest::Present, FlacDigest::Missing] {
                    let bytes = residual_fixtures::verification_surround_flac(
                        leading_id3,
                        optional_picture,
                        digest,
                        declared_frames,
                    );
                    let source = residual_fixtures::TemporarySource::new(&bytes);
                    let mut decoder = open(source.path()).expect("accepted prefixed FLAC opens");
                    assert_verified_surround_format(&decoder);
                    let initial = decoder.flac_md5_status();
                    let mut first = [0.0; 6];
                    assert_eq!(decoder.read_frames(&mut first).unwrap(), 1);
                    let partial = decoder.flac_md5_status();
                    let (suffix, result) = drain_surround_decoder(&mut decoder);
                    assert_eq!(std::fs::read(source.path()).unwrap(), bytes, "source bytes");
                    result.expect("complete prefixed FLAC decodes");
                    assert_eq!(first, reference[..6]);
                    assert_eq!(suffix, reference[6..], "every remaining source sample");
                    let (expected_initial, expected_final) = match digest {
                        FlacDigest::Present => (FlacMd5Status::Pending, FlacMd5Status::Verified),
                        FlacDigest::Missing => {
                            (FlacMd5Status::NoSignature, FlacMd5Status::NoSignature)
                        }
                        FlacDigest::Wrong => unreachable!(),
                    };
                    assert_eq!(initial, Some(expected_initial), "before any PCM is read");
                    assert_eq!(
                        partial,
                        Some(expected_initial),
                        "partial audio is not verified"
                    );
                    assert_eq!(decoder.flac_md5_status(), Some(expected_final));
                    let mut output = [42.0; 6];
                    assert_eq!(decoder.read_frames(&mut output).unwrap(), 0, "stable EOF");
                    assert_eq!(output, [42.0; 6], "EOF does not publish stale PCM");
                    assert_eq!(decoder.flac_md5_status(), Some(expected_final));
                }
            }
        }
    }
}

#[test]
fn prefixed_flac_keeps_md5_status_and_exact_pcm_with_known_or_unknown_length() {
    assert_prefixed_flac_verification_preserves_exact_pcm(FileDecoder::open);
}

#[test]
fn native_prefixed_flac_keeps_md5_status_and_exact_pcm_with_known_or_unknown_length() {
    assert_prefixed_flac_verification_preserves_exact_pcm(FileDecoder::open_native);
}

fn assert_prefixed_flac_md5_mismatch_is_terminal(open: OpenDecoder) {
    use super::FlacMd5Status;

    let reference = surround_pcm_reference(open);
    for leading_id3 in [false, true] {
        for optional_picture in [false, true] {
            for declared_frames in [120, 0] {
                let bytes = residual_fixtures::verification_surround_flac(
                    leading_id3,
                    optional_picture,
                    residual_fixtures::FlacDigest::Wrong,
                    declared_frames,
                );
                let source = residual_fixtures::TemporarySource::new(&bytes);
                let mut decoder =
                    open(source.path()).expect("wrong stored digest opens progressively");
                assert_verified_surround_format(&decoder);
                let initial = decoder.flac_md5_status();
                let (samples, result) = drain_surround_decoder(&mut decoder);
                assert_eq!(std::fs::read(source.path()).unwrap(), bytes, "source bytes");
                assert!(
                    matches!(result, Err(Error::FlacMd5Mismatch)),
                    "wrong source MD5 cannot become clean EOF: {result:?}"
                );
                assert_eq!(initial, Some(FlacMd5Status::Pending));
                assert_eq!(samples, reference, "MD5 uses the complete original PCM");
                assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Mismatch));
                for _ in 0..2 {
                    let mut output = [42.0; 6];
                    assert!(matches!(
                        decoder.read_frames(&mut output),
                        Err(Error::FlacMd5Mismatch)
                    ));
                    assert_eq!(output, [42.0; 6], "failed decoder publishes no stale PCM");
                    assert_eq!(decoder.flac_md5_status(), Some(FlacMd5Status::Mismatch));
                }
            }
        }
    }
}

#[test]
fn prefixed_flac_md5_mismatch_is_a_permanent_decode_error() {
    assert_prefixed_flac_md5_mismatch_is_terminal(FileDecoder::open);
}

#[test]
fn native_prefixed_flac_md5_mismatch_is_a_permanent_decode_error() {
    assert_prefixed_flac_md5_mismatch_is_terminal(FileDecoder::open_native);
}

fn assert_prefixed_flac_declared_frame_mismatch_is_terminal(
    open: OpenDecoder,
    declared_frames: u64,
) {
    use super::FlacMd5Status;
    use residual_fixtures::FlacDigest;

    let reference = surround_pcm_reference(open);
    for leading_id3 in [false, true] {
        for optional_picture in [false, true] {
            for digest in [FlacDigest::Present, FlacDigest::Missing] {
                let bytes = residual_fixtures::verification_surround_flac(
                    leading_id3,
                    optional_picture,
                    digest,
                    declared_frames,
                );
                let source = residual_fixtures::TemporarySource::new(&bytes);
                let mut decoder =
                    open(source.path()).expect("source count is checked progressively");
                assert_verified_surround_format(&decoder);
                let (samples, result) = drain_surround_decoder(&mut decoder);
                assert_eq!(std::fs::read(source.path()).unwrap(), bytes, "source bytes");
                let error = result.expect_err("incorrect source count cannot become clean EOF");
                assert!(matches!(error, Error::Decode(_)), "{error:?}");
                let expected_status = match digest {
                    FlacDigest::Present => FlacMd5Status::Pending,
                    FlacDigest::Missing => FlacMd5Status::NoSignature,
                    FlacDigest::Wrong => unreachable!(),
                };
                assert_eq!(decoder.flac_md5_status(), Some(expected_status));
                if declared_frames > 120 {
                    assert_eq!(
                        samples, reference,
                        "the complete audio precedes the early EOF"
                    );
                } else {
                    assert!(
                        samples.is_empty(),
                        "an excessive first packet is not published"
                    );
                }
                for _ in 0..2 {
                    let mut output = [42.0; 6];
                    let next = decoder.read_frames(&mut output).unwrap_err();
                    assert!(matches!(next, Error::Decode(_)), "{next:?}");
                    assert_eq!(output, [42.0; 6], "failed decoder publishes no stale PCM");
                    assert_eq!(decoder.flac_md5_status(), Some(expected_status));
                }
            }
        }
    }
}

#[test]
fn prefixed_flac_overstated_frame_count_never_becomes_clean_eof() {
    assert_prefixed_flac_declared_frame_mismatch_is_terminal(FileDecoder::open, 121);
}

#[test]
fn native_prefixed_flac_overstated_frame_count_never_becomes_clean_eof() {
    assert_prefixed_flac_declared_frame_mismatch_is_terminal(FileDecoder::open_native, 121);
}

#[test]
fn prefixed_flac_understated_frame_count_never_publishes_an_excessive_packet() {
    assert_prefixed_flac_declared_frame_mismatch_is_terminal(FileDecoder::open, 119);
}

#[test]
fn native_prefixed_flac_understated_frame_count_never_publishes_an_excessive_packet() {
    assert_prefixed_flac_declared_frame_mismatch_is_terminal(FileDecoder::open_native, 119);
}

#[test]
fn native_dsd_playback_uses_filtered_pcm_and_keeps_seek_units_in_pcm_frames() {
    for (name, channels, expected_frames) in [
        ("tone.dsf", 1, 2048),
        ("tone.dff", 1, 2048),
        ("stereo128.dsf", 2, 1024),
        ("stereo128.dff", 2, 1024),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-audio/tests/fixtures")
            .join(name);
        let mut decoder = FileDecoder::open_native(&path).unwrap();
        assert_eq!(decoder.sample_rate(), 88_200);
        assert_eq!(decoder.channels(), channels);
        let mut buffer = vec![0.0; 4096];
        let mut frames = 0;
        loop {
            let n = decoder.read_frames(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            frames += n;
        }
        assert_eq!(frames, expected_frames, "{name}");
        assert_eq!(decoder.flac_md5_status(), None);
        let mut decoder = FileDecoder::open_native(&path).unwrap();
        let seek_frames = (expected_frames / 2) as u64;
        let seek = decoder.skip_frames(seek_frames, || false).unwrap();
        assert_eq!(seek.frames, seek_frames);
        assert!(!seek.reached_eof);
        let mut remaining = 0;
        loop {
            let n = decoder.read_frames(&mut buffer).unwrap();
            if n == 0 {
                break;
            }
            remaining += n;
        }
        assert_eq!(
            remaining as u64,
            expected_frames as u64 - seek_frames,
            "{name}"
        );
    }
}
