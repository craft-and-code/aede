use super::{Dsp, DspError, PcmFormat, ProcessStats, gain_with_headroom_db, protect_output};

#[test]
fn headroom_caps_positive_gain_using_the_declared_peak() {
    let capped = gain_with_headroom_db(6.0, Some(0.75)).expect("valid peak");
    assert!((capped - 20.0 * (1.0_f32 / 0.75).log10()).abs() < 0.000_01);
    assert_eq!(gain_with_headroom_db(-6.0, Some(0.75)), Ok(-6.0));
}

#[test]
fn missing_peak_assumes_full_scale_and_an_overfull_peak_requires_attenuation() {
    assert_eq!(gain_with_headroom_db(12.0, None), Ok(0.0));
    assert_eq!(gain_with_headroom_db(-6.0, None), Ok(-6.0));
    let capped = gain_with_headroom_db(0.0, Some(2.0)).expect("valid peak");
    assert!((capped + 6.0206).abs() < 0.0001);
    assert_eq!(gain_with_headroom_db(12.0, Some(0.0)), Ok(12.0));
    assert_eq!(
        gain_with_headroom_db(0.0, Some(f32::NAN)),
        Err(DspError::InvalidPeak)
    );
    assert_eq!(
        gain_with_headroom_db(0.0, Some(-0.1)),
        Err(DspError::InvalidPeak)
    );
}

#[test]
fn output_guard_only_clamps_unexpected_overfull_samples() {
    let mut samples = [-1.5, -1.0, 0.25, 1.0, 1.25];
    assert_eq!(protect_output(&mut samples), Ok(2));
    assert_eq!(samples, [-1.0, -1.0, 0.25, 1.0, 1.0]);
    let mut bad = [0.25, f32::NAN, 1.5];
    assert_eq!(protect_output(&mut bad), Err(DspError::NonFiniteSample));
    assert_eq!(bad[0], 0.25);
    assert!(bad[1].is_nan());
    assert_eq!(bad[2], 1.5);
}

#[test]
fn playback_processing_returns_peak_before_its_final_safety_ceiling() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 1).expect("format"));
    dsp.set_gain_db(20.0, 0).expect("gain");
    let mut samples = [0.05, 0.2];
    let stats = dsp.process_for_output(&mut samples).expect("valid samples");
    assert_eq!(samples, [0.5, 1.0]);
    assert_eq!(stats.sample_peak, 2.0);
    assert_eq!(stats.overfull_samples, 1);
}

#[test]
fn pcm_format_requires_a_rate_and_channels() {
    assert_eq!(PcmFormat::new(0, 2), Err(DspError::InvalidFormat));
    assert_eq!(PcmFormat::new(48_000, 0), Err(DspError::InvalidFormat));
    let format = PcmFormat::new(48_000, 2).expect("valid format");
    assert_eq!(format.sample_rate(), 48_000);
    assert_eq!(format.channels(), 2);
}

#[test]
fn unity_gain_preserves_samples_and_reports_peak() {
    let mut dsp = Dsp::new(PcmFormat::new(44_100, 2).expect("format"));
    let mut samples = [0.25, -0.5, 1.25, -1.0];
    let stats = dsp.process(&mut samples).expect("valid samples");
    assert_eq!(samples, [0.25, -0.5, 1.25, -1.0]);
    assert_eq!(
        stats,
        ProcessStats {
            sample_peak: 1.25,
            overfull_samples: 1,
        }
    );
}

#[test]
fn gain_change_reaches_target_across_blocks_with_equal_channel_gain() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 2).expect("format"));
    dsp.set_gain_db(20.0, 4).expect("gain");
    let mut first = [0.1, -0.1, 0.1, -0.1];
    let mut second = [0.1, -0.1, 0.1, -0.1];
    dsp.process(&mut first).expect("first block");
    dsp.process(&mut second).expect("second block");
    for (actual, expected) in first.into_iter().zip([0.325, -0.325, 0.55, -0.55]) {
        assert!((actual - expected).abs() < 0.000_001);
    }
    for (actual, expected) in second.into_iter().zip([0.775, -0.775, 1.0, -1.0]) {
        assert!((actual - expected).abs() < 0.000_001);
    }
}

#[test]
fn negative_gain_attenuates_without_clipping() {
    let mut dsp = Dsp::new(PcmFormat::new(44_100, 1).expect("format"));
    dsp.set_gain_db(-6.020_6, 0).expect("gain");
    let mut samples = [1.0, -0.5];
    let stats = dsp.process(&mut samples).expect("valid samples");
    assert!((samples[0] - 0.5).abs() < 0.000_001);
    assert!((samples[1] + 0.25).abs() < 0.000_001);
    assert_eq!(stats.overfull_samples, 0);
}

#[test]
fn a_new_gain_ramp_starts_at_the_current_level() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 1).expect("format"));
    dsp.set_gain_db(20.0, 4).expect("first gain");
    let mut first = [0.1];
    dsp.process(&mut first).expect("first step");
    dsp.set_gain_db(0.0, 2).expect("replacement gain");
    let mut remainder = [0.1, 0.1];
    dsp.process(&mut remainder).expect("replacement ramp");
    assert!((remainder[0] - 0.2125).abs() < 0.000_001);
    assert_eq!(remainder[1], 0.1);
}

#[test]
fn rejected_buffer_does_not_change_samples_or_advance_ramp() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 2).expect("format"));
    dsp.set_gain_db(20.0, 2).expect("gain");
    let mut incomplete = [0.1];
    assert_eq!(dsp.process(&mut incomplete), Err(DspError::IncompleteFrame));
    assert_eq!(incomplete, [0.1]);
    let mut bad = [0.1, f32::NAN];
    assert_eq!(dsp.process(&mut bad), Err(DspError::NonFiniteSample));
    assert_eq!(bad[0], 0.1);
    assert!(bad[1].is_nan());
    let mut good = [0.1, -0.1];
    dsp.process(&mut good).expect("ramp still starts here");
    assert_eq!(good, [0.55, -0.55]);
}

#[test]
fn invalid_gain_keeps_previous_setting() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 1).expect("format"));
    dsp.set_gain_db(20.0, 0).expect("gain");
    assert_eq!(dsp.set_gain_db(f32::NAN, 0), Err(DspError::InvalidGain));
    assert_eq!(dsp.set_gain_db(1000.0, 0), Err(DspError::InvalidGain));
    let mut samples = [0.1];
    dsp.process(&mut samples).expect("previous gain remains");
    assert_eq!(samples, [1.0]);
}

#[test]
fn overflow_is_rejected_before_any_sample_is_changed() {
    let mut dsp = Dsp::new(PcmFormat::new(48_000, 1).expect("format"));
    dsp.set_gain_db(20.0, 0).expect("gain");
    let mut samples = [0.1, f32::MAX];
    assert_eq!(dsp.process(&mut samples), Err(DspError::SampleOverflow));
    assert_eq!(samples, [0.1, f32::MAX]);
}
