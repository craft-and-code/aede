use super::*;
use aede_dsp::Dsp;

#[test]
fn peak_limited_normalization_keeps_the_tone_headroom_reserve() {
    let tone = ToneControls::new(6.0, 0.0).unwrap();
    let format = PcmFormat::new(48_000, 1).unwrap();
    for source_peak in [None, Some(1.0)] {
        let selected = GainPlan {
            gain_db: 10.0,
            source_peak,
            label: "test normalization",
            peak_label: "test peak",
        };
        let gain = output_gain(Some(selected), tone).ok().unwrap();
        assert_eq!(
            gain, -6.0,
            "the normalization cap must not consume the EQ reserve"
        );
        let mut dsp = Dsp::new(format);
        dsp.set_gain_db(gain, 0).unwrap();
        dsp.set_tone(tone).unwrap();
        let mut low_frequency = vec![0.95; 48_000];
        let stats = dsp.process_for_output(&mut low_frequency).unwrap();
        assert_eq!(
            stats.overfull_samples, 0,
            "reserved bass headroom avoids clipping"
        );
    }
    assert_eq!(output_gain(None, tone).ok(), Some(-6.0));
}
