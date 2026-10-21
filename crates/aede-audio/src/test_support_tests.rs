use super::*;
#[test]
fn coincident_clock_stamps_never_overwrite_another_live_audio_fixture() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let first = Wave::with_stamp(&[0.25, -0.25], 2, stamp);
    let second = Wave::with_stamp(&[f32::NAN, 0.], 2, stamp);
    assert_ne!(
        first.0, second.0,
        "same-resolution timestamps must not share a file"
    );
    let mut decoder = crate::decode::PcmStreamDecoder::open(&first.0).unwrap();
    assert_eq!(decoder.next_chunk().unwrap().unwrap(), vec![0.25, -0.25]);
}
