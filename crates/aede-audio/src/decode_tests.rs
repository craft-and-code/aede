use super::*;
use crate::test_support::Wave;
#[test]
fn local_packets_preserve_samples_and_header_until_eof() {
    let input = vec![0.25, -0.5, 0.75, 0.];
    let wave = Wave::new(&input, 2);
    let mut decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    assert_eq!(decoder.sample_rate, 48_000);
    assert_eq!(decoder.channels, 2);
    let mut result = Vec::new();
    while let Some(packet) = decoder.next_chunk().unwrap() {
        result.extend(packet);
    }
    assert_eq!(result, input);
    assert!(decoder.next_chunk().unwrap().is_none());
}
#[test]
fn non_finite_samples_cannot_reach_a_device() {
    let wave = Wave::new(&[0., f32::NAN], 2);
    let mut decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    assert!(
        decoder
            .next_chunk()
            .unwrap_err()
            .to_string()
            .contains("non-finite")
    );
}
#[test]
fn invalid_header_and_non_regular_sources_are_errors() {
    assert!(PcmStreamDecoder::open(&std::env::temp_dir()).is_err());
    let wave = Wave::new(&[0., 0.], 2);
    std::fs::write(&wave.0, b"RIFF").unwrap();
    assert!(PcmStreamDecoder::open(&wave.0).is_err());
}

#[test]
fn retired_packet_read_cannot_consume_or_publish_the_next_packet() {
    let wave = Wave::new(&[0.25, -0.25], 2);
    let mut decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    assert!(
        decoder
            .next_chunk_admitted(|| Err(DecodeError("retired".into())))
            .is_err()
    );
    assert_eq!(decoder.next_chunk().unwrap().unwrap(), vec![0.25, -0.25]);
}

#[path = "decode_test_fixtures.rs"]
mod fixtures;
use fixtures::{Source, flac_with_metadata, matroska_with_timestamp_scale, native_flac};

#[test]
fn native_alac_uses_codec_clock_channels_and_exact_original_samples() {
    for (name, rate, channels, bits, frames) in [
        ("mono16", 44_100, 1usize, 16, 10_001usize),
        ("stereo24", 96_000, 2, 24, 48_013),
        ("stereo24-clock192k", 96_000, 2, 24, 48_013),
        ("stereo24-clock48k", 96_000, 2, 24, 48_013),
        ("surround16-layout", 48_000, 6, 16, 10_001),
        ("layout3-16", 48_000, 3, 16, 10_001),
        ("layout4-16", 48_000, 4, 16, 10_001),
        ("layout5-16", 48_000, 5, 16, 10_001),
        ("layout7-16", 48_000, 7, 16, 10_001),
        ("layout8-16", 48_000, 8, 16, 10_001),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-alac")
            .join(format!("{name}.m4a"));
        let before = std::fs::read(&path).unwrap();
        let mut decoder = PcmStreamDecoder::open(&path).unwrap();
        assert_eq!((decoder.sample_rate, decoder.channels), (rate, channels));
        if channels > 2 {
            let mask = [0x007, 0x107, 0x607, 0x60f, 0x70f, 0x6cf][channels - 3];
            assert_eq!(decoder.channel_mask, Some(mask), "{name}");
        }
        let mut samples = Vec::new();
        while let Some(chunk) = decoder.next_chunk().unwrap() {
            samples.extend(chunk);
        }
        assert_eq!(samples.len(), frames * channels);
        for (index, sample) in samples.iter().enumerate() {
            let frame = index / channels;
            let channel = index % channels;
            let integer = ((frame * 104729 + channel * 49999) % (1usize << bits)) as i64
                - (1i64 << (bits - 1));
            assert_eq!(*sample, integer as f32 / (1u32 << (bits - 1)) as f32);
        }
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

#[test]
fn truncated_alac_audio_is_not_a_successful_shortened_track() {
    let original = include_bytes!("../tests/fixtures/native-alac/stereo24.m4a");
    let source = Source::new(&original[..original.len() - 2000]);
    if let Ok(mut decoder) = PcmStreamDecoder::open(source.path()) {
        let mut frames = 0;
        loop {
            match decoder.next_chunk() {
                Ok(Some(chunk)) => frames += chunk.len() / decoder.channels,
                Ok(None) => panic!("truncated ALAC completed with {frames} frames"),
                Err(_) => break,
            }
        }
    }
}

#[test]
fn corrupt_alac_packets_cannot_be_skipped_as_successful_audio() {
    let original = include_bytes!("../tests/fixtures/native-alac/stereo24.m4a");
    let audio = original
        .windows(4)
        .position(|bytes| bytes == b"mdat")
        .unwrap()
        + 4;
    for first_byte in [0xe0, 0xff, 0x00] {
        let mut bytes = original.to_vec();
        bytes[audio] = first_byte;
        let source = Source::new(&bytes);
        let mut decoder = PcmStreamDecoder::open(source.path()).unwrap();
        let mut frames = 0;
        loop {
            match decoder.next_chunk() {
                Ok(Some(chunk)) => frames += chunk.len() / decoder.channels,
                Ok(None) => panic!("corrupt ALAC completed with {frames} frames"),
                Err(_) => break,
            }
        }
        assert!(decoder.next_chunk().is_err());
    }
}

#[test]
fn alac_header_length_disagreement_is_terminal_in_both_directions() {
    let original = include_bytes!("../tests/fixtures/native-alac/stereo24.m4a");
    let header = original
        .windows(4)
        .position(|bytes| bytes == b"mdhd")
        .unwrap()
        + 4;
    assert_eq!(original[header], 0);
    for frames in [47913u32, 48113] {
        let mut bytes = original.to_vec();
        bytes[header + 16..header + 20].copy_from_slice(&frames.to_be_bytes());
        let source = Source::new(&bytes);
        let mut decoder = PcmStreamDecoder::open(source.path()).unwrap();
        loop {
            match decoder.next_chunk() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("inconsistent ALAC duration was accepted"),
                Err(_) => break,
            }
        }
        assert!(decoder.next_chunk().is_err());
    }
}

#[test]
fn alac_with_a_coarser_media_clock_cannot_complete_with_an_absent_packet() {
    let mut bytes = include_bytes!("../tests/fixtures/native-alac/stereo24-clock48k.m4a").to_vec();
    let sizes = bytes.windows(4).position(|b| b == b"stsz").unwrap() + 4;
    let count = u32::from_be_bytes(bytes[sizes + 8..sizes + 12].try_into().unwrap()) as usize;
    let last = sizes + 12 + (count - 1) * 4;
    let missing = u32::from_be_bytes(bytes[last..last + 4].try_into().unwrap()) as usize;
    bytes.truncate(bytes.len() - missing);
    let mdat = bytes.windows(4).position(|b| b == b"mdat").unwrap() - 4;
    let extent = (bytes.len() - mdat) as u32;
    bytes[mdat..mdat + 4].copy_from_slice(&extent.to_be_bytes());
    let source = Source::new(&bytes);
    let mut decoder = PcmStreamDecoder::open(source.path()).unwrap();
    let mut frames = 0;
    loop {
        match decoder.next_chunk() {
            Ok(Some(samples)) => frames += samples.len() / 2,
            Ok(None) => panic!("incomplete ALAC succeeded with {frames} frames"),
            Err(_) => break,
        }
    }
    assert!(decoder.next_chunk().is_err());
}

#[test]
fn alac_packet_pcm_length_must_match_its_media_duration_before_publication() {
    let original = include_bytes!("../tests/fixtures/native-alac/stereo24-clock48k.m4a");
    let sizes = original.windows(4).position(|b| b == b"stsz").unwrap() + 4;
    let count = u32::from_be_bytes(original[sizes + 8..sizes + 12].try_into().unwrap()) as usize;
    let packet_sizes: Vec<_> = (0..count)
        .map(|i| {
            let at = sizes + 12 + 4 * i;
            u32::from_be_bytes(original[at..at + 4].try_into().unwrap()) as usize
        })
        .collect();
    let audio = original.windows(4).position(|b| b == b"mdat").unwrap() + 4;
    let last = audio + packet_sizes[..count - 1].iter().sum::<usize>();
    let mut bytes = original[..audio].to_vec();
    bytes.extend_from_slice(&original[last..last + packet_sizes[count - 1]]);
    bytes.extend_from_slice(&original[audio + packet_sizes[0]..]);
    bytes[sizes + 12..sizes + 16].copy_from_slice(&(packet_sizes[count - 1] as u32).to_be_bytes());
    let extent = (bytes.len() - (audio - 8)) as u32;
    bytes[audio - 8..audio - 4].copy_from_slice(&extent.to_be_bytes());
    let source = Source::new(&bytes);
    let mut decoder = PcmStreamDecoder::open(source.path()).unwrap();
    assert!(
        decoder.next_chunk().is_err(),
        "a short packet cannot publish PCM under a longer media duration"
    );
    assert!(decoder.next_chunk().is_err());
}

#[test]
fn retired_alac_decode_cannot_publish_or_resume_buffered_audio() {
    for name in ["mono16", "stereo24"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/native-alac")
            .join(format!("{name}.m4a"));
        let mut decoder = PcmStreamDecoder::open(&path).unwrap();
        let mut checks = 0;
        assert!(
            decoder
                .next_chunk_admitted(|| {
                    checks += 1;
                    if checks == 3 {
                        Err(DecodeError("retired during ALAC decode".into()))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
        );
        assert!(decoder.next_chunk().is_err());
    }
}

fn decode_samples(path: &Path) -> Vec<f32> {
    let mut decoder = PcmStreamDecoder::open(path).unwrap();
    assert_eq!(decoder.sample_rate, 44_100);
    assert_eq!(decoder.channels, 1);
    let mut samples = Vec::new();
    while let Some(packet) = decoder.next_chunk().unwrap() {
        samples.extend(packet);
    }
    samples
}

#[test]
fn zero_rate_wave_returns_an_error_without_panicking_or_modifying_source() {
    let wave = Wave::new(&[0.25, -0.5], 1);
    let mut bytes = std::fs::read(&wave.0).unwrap();
    bytes[24..32].fill(0);
    std::fs::write(&wave.0, &bytes).unwrap();
    let result = PcmStreamDecoder::open(&wave.0);
    assert_eq!(std::fs::read(&wave.0).unwrap(), bytes);
    assert!(result.is_err());
}

#[test]
fn invalid_matroska_timestamps_return_errors_without_modifying_source() {
    for scale in [0, u64::from(u32::MAX) + 1] {
        let bytes = matroska_with_timestamp_scale(scale);
        let source = Source::new(&bytes);
        let result = PcmStreamDecoder::open(source.path());
        assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
        assert!(result.is_err(), "invalid timestamp scale {scale}");
    }
}

#[cfg(unix)]
#[test]
fn local_admission_refuses_a_fifo_without_waiting_for_a_writer() {
    const CHILD_PATH: &str = "AEDE_AUDIO_FIFO_TEST_PATH";
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        assert!(PcmStreamDecoder::open(Path::new(&path)).is_err());
        return;
    }

    let source = Source::new(&[]);
    std::fs::remove_file(source.path()).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(source.path())
            .status()
            .unwrap()
            .success()
    );
    // An isolated child can always be reaped if a blocking open regresses.
    let mut worker = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "decode::tests::local_admission_refuses_a_fifo_without_waiting_for_a_writer",
        ])
        .env(CHILD_PATH, source.path())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = worker.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            timed_out = true;
            worker.kill().unwrap();
            break worker.wait().unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert!(!timed_out, "a FIFO needs no media writer to be refused");
    assert!(status.success(), "FIFO refusal child failed: {status}");
}

#[test]
fn native_flac_preserves_real_pcm_and_source_bytes() {
    let bytes = native_flac();
    let source = Source::new(&bytes);
    let samples = decode_samples(source.path());
    assert_eq!(samples.len(), 44_100);
    assert_eq!(
        &samples[..12],
        &[0, 51, 102, 153, 203, 252, 301, 348, 394, 438, 480, 521]
            .map(|value| value as f32 / 32_768.)
    );
    assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
}

#[test]
fn native_flac_ignores_unused_picture_lengths_when_decoding_pcm() {
    let original = Source::new(&native_flac());
    let reference = decode_samples(original.path());
    let bodies = [
        [3_u32, 65_536].map(u32::to_be_bytes).concat(),
        [3_u32, 0, 65_536].map(u32::to_be_bytes).concat(),
        [3_u32, 0, 0, 0, 0, 0, 0, 65_536]
            .map(u32::to_be_bytes)
            .concat(),
    ];
    for body in bodies {
        for leading_id3 in [false, true] {
            let bytes = flac_with_metadata(6, &body, leading_id3);
            let source = Source::new(&bytes);
            assert_eq!(decode_samples(source.path()), reference);
            assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
        }
    }
}

#[test]
fn native_flac_ignores_unused_comment_lengths_when_decoding_pcm() {
    let original = Source::new(&native_flac());
    let reference = decode_samples(original.path());
    let bodies = [
        65_536_u32.to_le_bytes().to_vec(),
        [0_u32, 1, 65_536].map(u32::to_le_bytes).concat(),
    ];
    for body in bodies {
        for leading_id3 in [false, true] {
            let bytes = flac_with_metadata(4, &body, leading_id3);
            let source = Source::new(&bytes);
            assert_eq!(decode_samples(source.path()), reference);
            assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
        }
    }
}

#[test]
fn native_flac_refuses_truncated_outer_metadata_without_modifying_source() {
    for leading_id3 in [false, true] {
        let mut bytes = flac_with_metadata(6, &[0; 8], leading_id3);
        // The final metadata body extends past the complete source, including audio.
        let metadata_header = 42 + usize::from(leading_id3) * 10;
        bytes[metadata_header + 1..metadata_header + 4].fill(0xff);
        let source = Source::new(&bytes);
        let result = PcmStreamDecoder::open(source.path());
        assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
        assert!(result.is_err());
    }
}

#[test]
fn native_flac_behind_junk_preserves_pcm_without_parsing_optional_picture_lengths() {
    let original = Source::new(&native_flac());
    let reference = decode_samples(original.path());
    let body = [3_u32, 65_536].map(u32::to_be_bytes).concat();
    let mut bytes = b"harmless leading bytes\0\x11\x22\x33".to_vec();
    bytes.extend(flac_with_metadata(6, &body, false));
    let source = Source::new(&bytes);
    assert_eq!(decode_samples(source.path()), reference);
    assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
}

#[test]
fn native_flac_behind_multiple_id3_headers_preserves_pcm_without_parsing_optional_picture_lengths()
{
    let original = Source::new(&native_flac());
    let reference = decode_samples(original.path());
    let body = [3_u32, 65_536].map(u32::to_be_bytes).concat();
    let mut bytes = b"ID3\x04\0\0\0\0\0\0ID3\x04\0\0\0\0\0\0".to_vec();
    bytes.extend(flac_with_metadata(6, &body, true));
    let source = Source::new(&bytes);
    assert_eq!(decode_samples(source.path()), reference);
    assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
}

#[test]
fn packet_reader_panic_returns_a_terminal_error_without_reusing_previous_pcm() {
    let bytes = native_flac();
    let source = Source::new(&bytes);
    let mut decoder = PcmStreamDecoder::open(source.path()).unwrap();
    assert!(!decoder.next_chunk().unwrap().unwrap().is_empty());
    let (mut decoder, calls) = fixtures::panic_on_next_packet(decoder);
    for _ in 0..3 {
        assert!(decoder.next_chunk().is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
    assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
}

#[test]
fn codec_panic_returns_a_terminal_error_without_reusing_previous_pcm() {
    let bytes = native_flac();
    let source = Source::new(&bytes);
    let mut decoder = PcmStreamDecoder::open(source.path()).unwrap();
    assert!(!decoder.next_chunk().unwrap().unwrap().is_empty());
    let (mut decoder, calls) = fixtures::panic_on_next_decode(decoder);
    for _ in 0..3 {
        assert!(decoder.next_chunk().is_err());
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
    assert_eq!(std::fs::read(source.path()).unwrap(), bytes);
}

#[test]
fn admission_panic_remains_visible_without_consuming_the_next_packet() {
    let wave = Wave::new(&[0.25, -0.25], 2);
    let mut decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decoder.next_chunk_admitted(|| panic!("controlled admission panic"))
    }));
    assert!(result.is_err());
    assert_eq!(decoder.next_chunk().unwrap().unwrap(), vec![0.25, -0.25]);
}
