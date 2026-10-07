use super::IntegerPcmFormat;
use super::PcmStreamFormat;
use aede_dsp::ChannelLayout;

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

#[test]
fn integer_source_format_keeps_valid_precision_and_channel_identity() {
    for bits in [16, 24] {
        for layout in [ChannelLayout::MONO, ChannelLayout::STEREO] {
            let format = IntegerPcmFormat::new(96_000, layout, bits).unwrap();
            assert_eq!(format.sample_rate(), 96_000);
            assert_eq!(format.bits_per_sample(), bits);
            assert_eq!(format.channels(), layout.channels());
            assert_eq!(format.layout(), layout);
        }
    }
}

#[test]
fn integer_source_format_refuses_unknown_layout_depth_or_rate() {
    assert!(IntegerPcmFormat::new(0, ChannelLayout::MONO, 16).is_err());
    for bits in [0, 8, 20, 32] {
        assert!(IntegerPcmFormat::new(48_000, ChannelLayout::STEREO, bits).is_err());
    }
    for layout in [
        ChannelLayout::Unknown(2),
        ChannelLayout::from_mask(0x3f).unwrap(),
    ] {
        assert!(IntegerPcmFormat::new(48_000, layout, 24).is_err());
    }
}
