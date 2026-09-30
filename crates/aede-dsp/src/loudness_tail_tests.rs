//! Finite-programme regressions for delayed true-peak interpolation output.
//!
//! These are independently generated boundary tests, not EBU archive cases.

use super::{LoudnessProgramme, Measurement};
use crate::PcmFormat;
use crate::reference_signals::{sine, stereo};

fn final_burst(rate: u32) -> Vec<f32> {
    let mut samples = sine(rate, rate as usize, 0.001, 1000.0, 0.0);
    samples.extend(sine(rate, 4, 0.5, f64::from(rate) / 4.0, 45.0));
    samples
}

fn programme(rate: u32, channels: u16, mono: &[f32]) -> LoudnessProgramme {
    let mut meter = LoudnessProgramme::new();
    let samples = if channels == 2 {
        stereo(mono)
    } else {
        mono.to_vec()
    };
    let format = PcmFormat::new(rate, channels).expect("tail reference format");
    for block in samples.chunks(usize::from(channels) * 1031) {
        meter.push(format, block).expect("tail reference samples");
    }
    meter
}

fn finished(meter: &LoudnessProgramme) -> Measurement {
    meter
        .measurement()
        .expect("tail reference measurement")
        .expect("complete quiet-tone gates before the final burst")
}

fn assert_final_burst_peak(rate: u32) {
    let mono = final_burst(rate);
    for channels in [1, 2] {
        let original = programme(rate, channels, &mono);
        // Zero extension lets all pending FIR outputs become observable. This
        // comparison uses no assumed absolute true-peak value for the burst.
        let mut padded = mono.clone();
        padded.extend([0.0; 64]);
        let reference = finished(&programme(rate, channels, &padded));
        let actual = finished(&original);
        let reference_peak = reference.true_peak.expect("oversampled reference peak");
        let actual_peak = actual.true_peak.expect("oversampled finite-programme peak");
        assert!(
            (actual_peak - reference_peak).abs() < 0.00001,
            "rate {rate}, channels {channels}: final peak {actual_peak} vs zero-extended {reference_peak}"
        );
        assert_eq!(actual.integrated_lufs, reference.integrated_lufs);
        // Measuring again must not add silent programme frames or mutate state.
        assert_eq!(finished(&original), actual);
    }
}

#[test]
fn a_48khz_programme_retains_inter_sample_peaks_from_its_last_frames() {
    assert_final_burst_peak(48_000);
}

#[test]
fn a_96khz_programme_retains_inter_sample_peaks_from_its_last_frames() {
    assert_final_burst_peak(96_000);
}

#[test]
fn a_format_boundary_keeps_true_peak_from_the_previous_sections_tail() {
    let mono = final_burst(48_000);
    let mut padded = mono.clone();
    padded.extend([0.0; 64]);
    let reference_peak = finished(&programme(48_000, 1, &padded))
        .true_peak
        .expect("oversampled previous-section reference");

    let mut mixed = programme(48_000, 1, &mono);
    mixed
        .push(
            PcmFormat::new(96_000, 1).expect("new section format"),
            &sine(96_000, 96_000, 0.001, 1000.0, 0.0),
        )
        .expect("second quiet section");
    let mixed_peak = finished(&mixed)
        .true_peak
        .expect("oversampled mixed-programme peak");
    assert!(
        (mixed_peak - reference_peak).abs() < 0.00001,
        "previous-section final peak {reference_peak} was lost: {mixed_peak}"
    );
}

#[test]
fn resolving_a_peak_tail_does_not_complete_an_incomplete_loudness_gate() {
    let rate = 48_000;
    let mut mono = sine(rate, rate as usize * 399 / 1000, 0.001, 1000.0, 0.0);
    mono.extend(sine(rate, 4, 0.5, f64::from(rate) / 4.0, 45.0));
    let meter = programme(rate, 2, &mono);
    assert_eq!(meter.measurement().expect("incomplete first gate"), None);
}
