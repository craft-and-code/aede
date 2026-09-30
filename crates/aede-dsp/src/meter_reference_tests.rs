//! Independently synthesized true-peak checks from EBU Tech 3341 Table 1.
//! https://tech.ebu.ch/docs/tech/tech3341.pdf

use super::*;
use crate::loudness::LoudnessProgramme;
use crate::reference_signals::{amplitude, sine, stereo};

// Mathematical definitions, not samples copied from the EBU test archive.
const PEAK_CASES: [(u8, f64, f32, f64, f32); 5] = [
    (15, 4.0, 0.50, 0.0, -6.0),
    (16, 4.0, 0.50, 45.0, -6.0),
    (17, 6.0, 0.50, 60.0, -6.0),
    (18, 8.0, 0.50, 67.5, -6.0),
    (19, 4.0, 1.41, 45.0, 3.0),
];

fn peak_tone(rate: u32, divisor: f64, level: f32, phase: f64) -> Vec<f32> {
    let frames = rate as usize / 2;
    let mut samples = sine(rate, frames, level, f64::from(rate) / divisor, phase);
    let fade = f64::from(rate) / 100.0;
    for (index, sample) in samples.iter_mut().enumerate() {
        let ramp = (index.min(frames - index - 1) as f64 / fade).min(1.0);
        *sample *= ramp as f32;
    }
    stereo(&samples)
}

fn observe(format: PcmFormat, samples: &[f32], block_frames: usize) -> OutputSnapshot {
    let mut meter = OutputMeter::new(format).expect("reference output meter");
    for block in samples.chunks(block_frames * usize::from(format.channels())) {
        meter
            .observe(
                block,
                ProcessStats {
                    sample_peak: block
                        .iter()
                        .fold(0.0_f32, |peak, value| peak.max(value.abs())),
                    overfull_samples: 0,
                },
            )
            .expect("guarded reference frames");
    }
    meter.snapshot().expect("reference snapshot")
}

fn assert_peak(case: u8, rate: u32, actual: f32, expected_db: f32) {
    let actual_db = 20.0 * actual.log10();
    assert!(
        (expected_db - 0.4..=expected_db + 0.2).contains(&actual_db),
        "TC{case} at {rate} Hz: expected {expected_db:+.1} dBTP [-0.4,+0.2], measured {actual_db:+.6}"
    );
}

#[test]
fn ebu_3341_cases_15_to_19_have_absolute_output_and_source_true_peaks() {
    // The published suite uses 48 kHz. Other rates exercise the independent
    // mathematical definitions through ebur128's 4x and 2x interpolation paths.
    for rate in [44_100, 48_000, 96_000] {
        let format = PcmFormat::new(rate, 2).expect("reference format");
        for (case, divisor, level, phase, expected) in PEAK_CASES {
            let samples = peak_tone(rate, divisor, level, phase);
            for block_frames in [37, 4096] {
                let measured = observe(format, &samples, block_frames);
                assert_peak(
                    case,
                    rate,
                    measured.output_true_peak.expect("oversampling"),
                    expected,
                );
                assert_eq!(measured.frames, rate as u64 / 2);
                assert_eq!(measured.guarded_samples, 0);
                let mut source = LoudnessProgramme::new();
                for block in samples.chunks(block_frames * 2) {
                    source.push(format, block).expect("source reference frames");
                }
                let source_peak = source
                    .measurement()
                    .expect("source snapshot")
                    .expect("complete source gate")
                    .true_peak
                    .expect("source oversampling");
                assert_peak(case, rate, source_peak, expected);
                if case == 19 {
                    assert!(measured.output_sample_peak < 1.0);
                    assert!(measured.output_true_peak.expect("peak") > 1.3);
                }
            }
        }
    }
}

#[test]
fn known_attenuation_changes_true_peak_by_the_same_decibels() {
    let format = PcmFormat::new(48_000, 2).expect("format");
    let samples = peak_tone(48_000, 4.0, 0.5, 45.0);
    let original = observe(format, &samples, 1031)
        .output_true_peak
        .expect("peak");
    for gain in [-6.0, -20.0, -60.0] {
        let adjusted = samples
            .iter()
            .map(|sample| *sample * amplitude(gain))
            .collect::<Vec<_>>();
        let peak = observe(format, &adjusted, 4096)
            .output_true_peak
            .expect("scaled peak");
        let delta_db = 20.0 * (peak / original).log10();
        assert!(
            (f64::from(delta_db) - gain).abs() < 0.005,
            "gain {gain}: {delta_db}"
        );
    }
}

#[test]
fn unoversampled_high_rate_reference_remains_sample_peak_only() {
    for rate in [192_000, 384_000] {
        let format = PcmFormat::new(rate, 2).expect("format");
        let samples = peak_tone(rate, 4.0, 1.41, 45.0);
        let measured = observe(format, &samples, 1031);
        assert_eq!(measured.output_true_peak, None);
        assert!((measured.output_sample_peak - 1.41 / 2.0_f32.sqrt()).abs() < 0.0001);
        assert_eq!(measured.guarded_samples, 0);
    }
}

#[test]
fn output_snapshot_includes_final_inter_sample_peaks_without_extra_frames() {
    for rate in [48_000, 96_000] {
        for channels in [1, 2] {
            for burst_frames in [1, 4, 12] {
                let mut mono = sine(rate, rate as usize, 0.001, 1000.0, 0.0);
                mono.extend(sine(rate, burst_frames, 0.5, f64::from(rate) / 4.0, 45.0));
                let samples = if channels == 2 { stereo(&mono) } else { mono };
                let format = PcmFormat::new(rate, channels).expect("output tail format");
                let actual = observe(format, &samples, 37);
                let mut padded = samples.clone();
                padded.resize(padded.len() + 64 * usize::from(channels), 0.0);
                let expected = observe(format, &padded, 4096);
                assert_eq!(actual.frames, samples.len() as u64 / u64::from(channels));
                assert_eq!(actual.guarded_samples, 0);
                assert_eq!(actual.output_sample_peak, expected.output_sample_peak);
                assert!(
                    (actual.output_true_peak.expect("final estimate")
                        - expected.output_true_peak.expect("zero-extended estimate"))
                    .abs()
                        < 0.00001,
                    "rate {rate}, channels {channels}, burst {burst_frames}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
}
