use super::PcmStreamFormat;

#[test]
fn output_format_refuses_unusable_pcm() {
    assert!(PcmStreamFormat::new(0, 2).is_err());
    assert!(PcmStreamFormat::new(48_000, 0).is_err());
    assert_eq!(
        PcmStreamFormat::new(48_000, 2).unwrap().sample_rate(),
        48_000
    );
    assert_eq!(PcmStreamFormat::new(48_000, 2).unwrap().channels(), 2);
}
