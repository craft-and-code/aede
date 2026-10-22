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
