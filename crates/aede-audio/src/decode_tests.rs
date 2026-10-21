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
