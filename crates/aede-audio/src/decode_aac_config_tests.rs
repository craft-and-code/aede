use super::*;

fn source(bytes: &[u8]) -> CodecParameters {
    let mut p = CodecParameters::new();
    p.codec = CODEC_TYPE_AAC;
    p.extra_data = Some(bytes.into());
    p
}
#[test]
fn lc_configuration_supplies_codec_properties_without_inventing_pcm_precision() {
    for (bytes, rate, channels) in [
        (&[0x12, 0x08, 0x56, 0xe5, 0][..], 44100, 1),
        (&[0x11, 0x90][..], 48000, 2),
    ] {
        let p = parameters(&source(bytes)).unwrap();
        assert_eq!(p.sample_rate, Some(rate));
        assert_eq!(p.channels.unwrap().count(), channels);
        assert_eq!(p.bits_per_sample, None);
    }
}
#[test]
fn truncated_and_unqualified_aac_profiles_are_not_guessed() {
    for bytes in [
        &[][..],
        &[0x12][..],
        &[0x2a, 0x10][..],
        &[0x12, 0x00][..],
        &[0x12, 0x0c][..],
        &[0x12, 0x08, 0x56, 0xe5, 0x80][..],
    ] {
        assert!(parameters(&source(bytes)).is_err(), "{bytes:?}");
    }
}
