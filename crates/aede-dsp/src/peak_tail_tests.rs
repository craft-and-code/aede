use ebur128::{EbuR128, Mode};

use super::{PeakTail, TAIL_FRAMES};
use crate::PcmFormat;
use crate::reference_signals::{sine, stereo};

fn samples_for_channels(mono: &[f32], channels: u16) -> Vec<f32> {
    if channels == 2 {
        stereo(mono)
    } else {
        mono.to_vec()
    }
}

fn observed(rate: u32, channels: u16, samples: &[f32], block_frames: usize) -> (EbuR128, PeakTail) {
    let format = PcmFormat::new(rate, channels).expect("tail format");
    let mut meter = EbuR128::new(u32::from(channels), rate, Mode::TRUE_PEAK).expect("peak meter");
    let mut tail = PeakTail::new(format);
    for block in samples.chunks(usize::from(channels) * block_frames) {
        meter.add_frames_f32(block).expect("complete finite frames");
        tail.push(block);
    }
    (meter, tail)
}

fn maximum(meter: &EbuR128) -> f32 {
    (0..meter.channels())
        .map(|channel| meter.true_peak(channel).expect("channel peak") as f32)
        .fold(0.0_f32, f32::max)
}

fn zero_extended_peak(rate: u32, channels: u16, samples: &[f32]) -> f32 {
    let mut meter =
        EbuR128::new(u32::from(channels), rate, Mode::TRUE_PEAK).expect("reference meter");
    meter.add_frames_f32(samples).expect("reference source PCM");
    meter
        .add_frames_f32(&vec![0.0; TAIL_FRAMES * usize::from(channels)])
        .expect("reference zero extension");
    maximum(&meter)
}

fn assert_peak(actual: Option<f32>, expected: f32) {
    let actual = actual.expect("oversampled peak is available");
    assert!(
        (actual - expected).abs() < 0.00001,
        "expected complete FIR peak {expected}, measured {actual}"
    );
}

#[test]
fn even_short_nonzero_signals_resolve_all_delayed_inter_sample_peaks() {
    for rate in [44_100, 48_000, 96_000, 176_400] {
        for frames in [1, 2, 3, 4, 8, 20] {
            let mono = sine(rate, frames, 0.5, f64::from(rate) / 4.0, 45.0);
            for channels in [1, 2] {
                let samples = samples_for_channels(&mono, channels);
                let expected = zero_extended_peak(rate, channels, &samples);
                let (meter, tail) = observed(rate, channels, &samples, 3);
                assert_peak(tail.peak(&meter).expect("resolved tail"), expected);
            }
        }
    }
}

#[test]
fn retained_tail_matches_a_complete_signal_across_input_block_boundaries() {
    for rate in [48_000, 96_000] {
        let mut mono = sine(rate, rate as usize / 10, 0.001, 1000.0, 0.0);
        mono.extend(sine(rate, 4, 0.5, f64::from(rate) / 4.0, 45.0));
        for channels in [1, 2] {
            let samples = samples_for_channels(&mono, channels);
            let expected = zero_extended_peak(rate, channels, &samples);
            for block_frames in [1, 7, 31, 1031, mono.len()] {
                let (meter, tail) = observed(rate, channels, &samples, block_frames);
                assert_peak(tail.peak(&meter).expect("block-independent tail"), expected);
            }
        }
    }
}

#[test]
fn a_steady_tone_ending_does_not_inherit_the_temporary_seed_startup_artifact() {
    let rate = 48_000;
    let mut mono = sine(rate, rate as usize / 10, 0.5, 500.0, 0.0);
    for (frame, sample) in mono.iter_mut().enumerate() {
        *sample *= (frame as f32 / 480.0).min(1.0);
    }
    for channels in [1, 2] {
        let samples = samples_for_channels(&mono, channels);
        let expected = zero_extended_peak(rate, channels, &samples);
        let seed = &samples[samples.len() - TAIL_FRAMES * usize::from(channels)..];
        let naive = zero_extended_peak(rate, channels, seed);
        // The independently observed counterexample ensures that using the
        // temporary meter's lifetime max would make this test fail.
        assert!(naive > expected + 0.05, "seed artifact was not present");
        let (meter, tail) = observed(rate, channels, &samples, 7);
        assert_peak(tail.peak(&meter).expect("only final FIR output"), expected);
    }
}

#[test]
fn snapshot_is_repeatable_and_does_not_insert_silence_before_future_input() {
    let rate = 48_000;
    let mut samples = sine(rate, rate as usize / 10, 0.001, 1000.0, 0.0);
    samples.extend(sine(rate, 4, 0.5, f64::from(rate) / 4.0, 45.0));
    let (mut meter, mut tail) = observed(rate, 1, &samples, 31);
    let original_peak = maximum(&meter);
    let before = tail.samples.clone();
    let first = tail.peak(&meter).expect("first snapshot");
    assert_eq!(tail.peak(&meter).expect("second snapshot"), first);
    assert_eq!(maximum(&meter), original_peak);
    assert_eq!(tail.samples, before);

    let continuation = sine(rate, 16, 0.5, f64::from(rate) / 4.0, 45.0);
    meter
        .add_frames_f32(&continuation)
        .expect("future source input");
    tail.push(&continuation);
    samples.extend(continuation);
    assert_peak(
        tail.peak(&meter).expect("snapshot after continuation"),
        zero_extended_peak(rate, 1, &samples),
    );
}

#[test]
fn peak_tail_keeps_a_larger_peak_from_earlier_audio() {
    let rate = 48_000;
    let mut samples = sine(rate, 4800, 0.8, 1000.0, 0.0);
    samples.extend(sine(rate, 4, 0.2, f64::from(rate) / 4.0, 45.0));
    let (meter, tail) = observed(rate, 1, &samples, 1031);
    let earlier = maximum(&meter);
    assert!(earlier >= 0.8);
    assert_peak(tail.peak(&meter).expect("preserved earlier peak"), earlier);
}

#[test]
fn unsupported_high_rates_retain_no_pcm_and_do_not_claim_true_peak() {
    for rate in [192_000, 384_000] {
        let (meter, tail) = observed(rate, 2, &[0.5, -0.5, -0.3, 0.3], 1);
        assert_eq!(tail.peak(&meter).expect("unsupported oversampling"), None);
        assert_eq!(tail.samples.capacity(), 0);
        assert!(tail.samples.is_empty());
    }
}

#[test]
fn missing_true_peak_mode_is_reported_instead_of_relabeling_a_sample_peak() {
    let format = PcmFormat::new(48_000, 1).expect("format");
    let meter = EbuR128::new(1, 48_000, Mode::I).expect("loudness-only meter");
    let mut tail = PeakTail::new(format);
    tail.push(&[0.5]);
    assert_eq!(tail.peak(&meter), Err(ebur128::Error::InvalidMode));
}
