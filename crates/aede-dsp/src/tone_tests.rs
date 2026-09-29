use std::f64::consts::TAU;

use super::ToneControls;
use crate::{Dsp, DspError, PcmFormat};

fn response_db(controls: ToneControls, frequency: f64) -> f64 {
    let rate = 48_000.0;
    let mut dsp = Dsp::new(PcmFormat::new(rate as u32, 1).expect("format"));
    dsp.set_tone(controls).expect("valid tone");
    let mut samples = (0..48_000)
        .map(|frame| (0.1 * (TAU * frequency * frame as f64 / rate).sin()) as f32)
        .collect::<Vec<_>>();
    dsp.process(&mut samples).expect("valid signal");
    let output_energy: f64 = samples[24_000..]
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum();
    let input_energy: f64 = (24_000..48_000)
        .map(|frame| (0.1 * (TAU * frequency * frame as f64 / rate).sin()).powi(2))
        .sum();
    10.0 * (output_energy / input_energy).log10()
}

#[test]
fn bass_shelf_boosts_low_frequencies_without_shifting_treble() {
    let tone = ToneControls::new(6.0, 0.0).expect("tone");
    assert!((response_db(tone, 40.0) - 6.0).abs() < 0.5);
    assert!(response_db(tone, 2_000.0).abs() < 0.1);
}

#[test]
fn treble_shelf_cuts_high_frequencies_without_shifting_bass() {
    let tone = ToneControls::new(0.0, -6.0).expect("tone");
    assert!((response_db(tone, 16_000.0) + 6.0).abs() < 0.5);
    assert!(response_db(tone, 100.0).abs() < 0.1);
}

#[test]
fn flat_reset_bypasses_filters_and_keeps_the_next_block_exact() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 2).expect("format"));
    dsp.set_tone(ToneControls::new(9.0, -3.0).expect("tone"))
        .expect("filter");
    let mut first = [0.5, 0.0, -0.5, 0.0];
    dsp.process(&mut first).expect("filtered");
    dsp.set_tone(ToneControls::FLAT).expect("reset");
    let mut next = [0.25, -0.125, -0.5, 0.75];
    dsp.process(&mut next).expect("bypass");
    assert_eq!(next, [0.25, -0.125, -0.5, 0.75]);
}

#[test]
fn filter_state_is_separate_per_channel_and_across_blocks() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let tone = ToneControls::new(6.0, 6.0).expect("tone");
    let mut whole = Dsp::new(format);
    whole.set_tone(tone).expect("filter");
    let mut split = Dsp::new(format);
    split.set_tone(tone).expect("filter");
    let mut signal = vec![0.0; 2_000];
    signal[0] = 0.25;
    let mut pieces = signal.clone();
    whole.process(&mut signal).expect("whole block");
    for block in pieces.chunks_mut(74) {
        split.process(block).expect("partial block");
    }
    assert_eq!(pieces, signal);
    assert!(
        signal
            .as_chunks::<2>()
            .0
            .iter()
            .all(|frame| frame[1] == 0.0)
    );
}

#[test]
fn gain_limits_and_preamp_are_explicit() {
    assert_eq!(
        ToneControls::new(6.0, 3.0).expect("tone").safe_preamp_db(),
        -9.0
    );
    assert_eq!(
        ToneControls::new(-6.0, -3.0)
            .expect("tone")
            .safe_preamp_db(),
        0.0
    );
    assert_eq!(ToneControls::new(12.1, 0.0), Err(DspError::InvalidTone));
    assert_eq!(ToneControls::new(0.0, f32::NAN), Err(DspError::InvalidTone));
}

#[test]
fn shelves_remain_defined_at_low_sample_rates() {
    let mut dsp = Dsp::new(PcmFormat::new(8_000, 1).expect("format"));
    dsp.set_tone(ToneControls::new(12.0, 12.0).expect("tone"))
        .expect("filter");
    let mut samples = vec![0.0; 8_000];
    samples[0] = 0.25;
    dsp.process(&mut samples).expect("signal");
    assert!(samples.iter().all(|sample| sample.is_finite()));
}
