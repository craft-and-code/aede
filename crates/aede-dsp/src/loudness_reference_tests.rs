//! Absolute checks against independently synthesized EBU Tech 3341 parameters.
//!
//! Table 1, cases 1–5, specifies +/- 0.1 LU for integrated loudness. The archive
//! of official EBU test recordings is not redistributed or used by these tests.
//! https://tech.ebu.ch/docs/tech/tech3341.pdf

use super::{LoudnessProgramme, Measurement};
use crate::PcmFormat;
use crate::reference_signals::{amplitude, sine, stepped_stereo, stereo};

const LU_TOLERANCE: f32 = 0.1;

fn measure(rate: u32, channels: u16, samples: &[f32]) -> Option<Measurement> {
    let mut programme = LoudnessProgramme::new();
    let format = PcmFormat::new(rate, channels).expect("reference PCM format");
    // Deliberately cross the meter's 100 ms and 400 ms window boundaries.
    for block in samples.chunks(usize::from(channels) * 1031) {
        programme.push(format, block).expect("reference PCM frames");
    }
    programme.measurement().expect("reference measurement")
}

fn assert_lufs(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= LU_TOLERANCE,
        "expected {expected:.4} +/- {LU_TOLERANCE} LUFS, measured {actual:.6} LUFS"
    );
}

fn assert_stereo_steps(steps: &[(f64, f64)], expected: f32) {
    let samples = stepped_stereo(48_000, steps);
    let measured = measure(48_000, 2, &samples).expect("audible reference programme");
    assert_lufs(measured.integrated_lufs, expected);
}

#[test]
fn ebu_3341_case_1_stereo_1khz_at_minus_23_has_absolute_lufs() {
    assert_stereo_steps(&[(20.0, -23.0)], -23.0);
}

#[test]
fn ebu_3341_case_2_stereo_1khz_at_minus_33_has_absolute_lufs() {
    assert_stereo_steps(&[(20.0, -33.0)], -33.0);
}

#[test]
fn ebu_3341_case_3_relative_gate_excludes_quiet_tone_sections() {
    assert_stereo_steps(&[(10.0, -36.0), (60.0, -23.0), (10.0, -36.0)], -23.0);
}

#[test]
fn ebu_3341_case_4_absolute_and_relative_gates_exclude_low_sections() {
    assert_stereo_steps(
        &[
            (10.0, -72.0),
            (10.0, -36.0),
            (60.0, -23.0),
            (10.0, -36.0),
            (10.0, -72.0),
        ],
        -23.0,
    );
}

#[test]
fn ebu_3341_case_5_integrates_energy_across_loud_and_quiet_steps() {
    assert_stereo_steps(&[(20.0, -26.0), (20.1, -20.0), (20.0, -26.0)], -23.0);
}

#[test]
fn absolute_gate_refuses_below_minus_70_but_keeps_above_threshold_audio() {
    let below = stepped_stereo(48_000, &[(3.0, -71.0)]);
    assert_eq!(measure(48_000, 2, &below), None);

    let above = stepped_stereo(48_000, &[(3.0, -69.0)]);
    assert_lufs(
        measure(48_000, 2, &above)
            .expect("tone above absolute gate")
            .integrated_lufs,
        -69.0,
    );
}

#[test]
fn silence_and_an_incomplete_first_gate_have_no_integrated_lufs() {
    assert_eq!(measure(48_000, 2, &[0.0; 96_000]), None);
    // Tech 3341 section 2.3 discards incomplete 400 ms gating blocks.
    let samples = sine(48_000, 48_000 * 399 / 1000, amplitude(-23.0), 1000.0, 0.0);
    assert_eq!(measure(48_000, 2, &stereo(&samples)), None);
}

#[test]
fn calibrated_mono_and_stereo_lufs_remain_absolute_across_sample_rates() {
    // Stereo calibration is from Tech 3341 section 2.9. Mono has half the
    // summed energy: -10*log10(2) LU relative to in-phase stereo (BS.1770).
    // These rates are additional coverage, not official downloaded test cases.
    let mono_expected = -23.0 - 10.0 * 2.0_f32.log10();
    for rate in [44_100, 48_000, 96_000, 192_000, 384_000] {
        let mono = sine(rate, rate as usize * 3, amplitude(-23.0), 1000.0, 0.0);
        let mono_measured = measure(rate, 1, &mono).expect("mono calibration tone");
        let stereo_measured = measure(rate, 2, &stereo(&mono)).expect("stereo calibration tone");
        assert_lufs(mono_measured.integrated_lufs, mono_expected);
        assert_lufs(stereo_measured.integrated_lufs, -23.0);
    }
}

#[test]
fn input_gain_changes_integrated_lufs_by_the_same_decibels() {
    let samples = stepped_stereo(48_000, &[(3.0, -33.0)]);
    let original = measure(48_000, 2, &samples).expect("original programme");
    let louder = samples
        .iter()
        .map(|sample| *sample * amplitude(10.0))
        .collect::<Vec<_>>();
    let scaled = measure(48_000, 2, &louder).expect("scaled programme");
    assert_lufs(original.integrated_lufs, -33.0);
    assert_lufs(scaled.integrated_lufs, -23.0);
    assert!((scaled.integrated_lufs - original.integrated_lufs - 10.0).abs() < 0.001);
}
