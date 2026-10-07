use aede_dsp::ChannelLayout;

use super::*;

#[test]
fn silent_acceptance_sources_retain_every_frame_and_channel_in_the_format_grid() {
    for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] {
        for bits in [16, 24] {
            for layout in [ChannelLayout::MONO, ChannelLayout::STEREO] {
                let format = IntegerPcmFormat::new(rate, layout, bits).unwrap();
                let fixture = SilentWav::new(format);
                let samples = fixture.verified_samples(format);
                assert_eq!(fixture.frames, rate as usize / 5);
                assert_eq!(
                    samples.len(),
                    (rate as usize / 5) * usize::from(format.channels())
                );
                assert!(samples.iter().all(|&sample| sample == 0));
                assert_eq!(std::fs::read(&fixture.path).unwrap(), fixture.bytes);
            }
        }
    }
}
