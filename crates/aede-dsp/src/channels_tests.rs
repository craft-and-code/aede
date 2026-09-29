use super::{ChannelLayout, StereoDownmixer};
use crate::{DspError, PcmFormat};

#[test]
fn a_known_mask_preserves_channel_positions_in_the_pcm_contract() {
    let layout = ChannelLayout::from_mask(0x60f).expect("5.1 side");
    let format = PcmFormat::with_layout(48_000, layout).expect("format");
    assert_eq!(format.channels(), 6);
    assert_eq!(format.layout(), layout);
    assert_eq!(layout.name(), "5.1 side (FL FR FC LFE SL SR)");
    assert_eq!(
        PcmFormat::new(48_000, 6).expect("count only").layout(),
        ChannelLayout::Unknown(6)
    );
}

#[test]
fn five_one_downmix_routes_each_speaker_and_omits_lfe() {
    let downmix =
        StereoDownmixer::new(ChannelLayout::from_mask(0x3f).expect("layout")).expect("matrix");
    let mut output = [0.0; 12];
    downmix
        .process(
            &[
                1.0, 0.0, 0.0, 0.0, 0.0, 0.0, // FL
                0.0, 1.0, 0.0, 0.0, 0.0, 0.0, // FR
                0.0, 0.0, 1.0, 0.0, 0.0, 0.0, // center
                0.0, 0.0, 0.0, 1.0, 0.0, 0.0, // LFE
                0.0, 0.0, 0.0, 0.0, 1.0, 0.0, // rear left
                0.0, 0.0, 0.0, 0.0, 0.0, 1.0, // rear right
            ],
            &mut output,
        )
        .expect("downmix");
    assert!(output[0] > 0.4 && output[1] == 0.0);
    assert!(output[2] == 0.0 && output[3] > 0.4);
    assert!((output[4] - output[5]).abs() < 0.000_001);
    assert!(output[4] > 0.25);
    assert_eq!(&output[6..8], &[0.0, 0.0]);
    assert!(output[8] > 0.25 && output[9] == 0.0);
    assert!(output[10] == 0.0 && output[11] > 0.25);
}

#[test]
fn coherent_full_scale_channels_do_not_overload_the_downmix() {
    for mask in [
        0xb, 0x7, 0xf, 0x33, 0x603, 0x37, 0x3f, 0x607, 0x60f, 0x637, 0x63f,
    ] {
        let layout = ChannelLayout::from_mask(mask).expect("layout");
        let downmix = StereoDownmixer::new(layout).expect("matrix");
        let source = vec![1.0; usize::from(layout.channels()) * 16];
        let mut output = [0.0; 32];
        downmix.process(&source, &mut output).expect("downmix");
        assert!(output.iter().all(|&sample| sample <= 1.0), "mask {mask:x}");
    }
}

#[test]
fn seven_one_keeps_side_and_rear_positions_on_their_own_stereo_sides() {
    let downmix =
        StereoDownmixer::new(ChannelLayout::from_mask(0x63f).expect("layout")).expect("matrix");
    let mut output = [0.0; 8];
    downmix
        .process(
            &[
                0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, // rear left
                0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, // rear right
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, // side left
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, // side right
            ],
            &mut output,
        )
        .expect("downmix");
    assert!(output[0] > 0.0 && output[1] == 0.0);
    assert!(output[2] == 0.0 && output[3] > 0.0);
    assert!(output[4] > 0.0 && output[5] == 0.0);
    assert!(output[6] == 0.0 && output[7] > 0.0);
}

#[test]
fn unknown_layout_and_invalid_blocks_are_rejected_without_output_changes() {
    assert!(matches!(
        StereoDownmixer::new(ChannelLayout::Unknown(6)),
        Err(DspError::InvalidLayout)
    ));
    assert!(matches!(
        StereoDownmixer::new(ChannelLayout::from_mask(0x207).expect("layout")),
        Err(DspError::InvalidLayout)
    ));
    let downmix =
        StereoDownmixer::new(ChannelLayout::from_mask(0x7).expect("layout")).expect("matrix");
    let mut output = [42.0; 2];
    assert_eq!(
        downmix.process(&[1.0, f32::NAN, 0.0], &mut output),
        Err(DspError::NonFiniteSample)
    );
    assert_eq!(output, [42.0; 2]);
    assert_eq!(
        downmix.process(&[1.0, 0.0], &mut output),
        Err(DspError::IncompleteFrame)
    );
    assert_eq!(output, [42.0; 2]);
}
