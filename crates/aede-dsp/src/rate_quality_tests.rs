use super::*;
use std::f64::consts::TAU;
use std::time::Instant;

#[path = "rate_quality_fixtures.rs"]
mod fixtures;
use fixtures::*;

// These product budgets are fixed before measurement, rather than fitted to results.
// The transition band above 80% of the lower Nyquist is measured separately.
// Rubato's f32 path need not match CamillaDSP's published f64 noise floor.
const PASSBAND_TOLERANCE_DB: f64 = 0.1;
const MIN_REJECTION_DB: f64 = 80.0;
const MAX_DELAY_ERROR_FRAMES: f64 = 1.0;
const IRREGULAR_CHUNKS: &[usize] = &[1, 101, 197, 1023, 4097, 2, 1024, 733];

fn assert_passband(rates: &[(u32, u32)]) {
    let started = Instant::now();
    let mut failures = Vec::new();
    for &(input_rate, output_rate) in rates {
        let lower_nyquist = f64::from(input_rate.min(output_rate)) / 2.0;
        let mut maximum_gain_error = 0.0_f64;
        let mut minimum_residual_rejection = f64::INFINITY;
        let mut maximum_delay_error = 0.0_f64;
        for frequency in [
            20.0,
            1_000.0,
            lower_nyquist * 0.1,
            lower_nyquist * 0.25,
            lower_nyquist * 0.5,
            lower_nyquist * 0.8,
        ] {
            let input = sine(input_rate, input_rate as usize, frequency);
            let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
            assert_eq!(output.len(), output_rate as usize);
            // 4096 output frames at both ends comfortably exclude these filters' support.
            let trim = 4096;
            let measurement = measure_tone(
                &output[trim..output.len() - trim],
                trim,
                output_rate,
                frequency,
            );
            let delay_frames =
                measurement.phase_error.abs() / (TAU * frequency / f64::from(output_rate));
            maximum_gain_error = maximum_gain_error.max(measurement.gain_db.abs());
            minimum_residual_rejection =
                minimum_residual_rejection.min(measurement.residual_rejection_db);
            maximum_delay_error = maximum_delay_error.max(delay_frames);
            if measurement.gain_db.abs() > PASSBAND_TOLERANCE_DB
                || measurement.residual_rejection_db < MIN_REJECTION_DB
                || delay_frames > MAX_DELAY_ERROR_FRAMES
            {
                failures.push(format!(
                    "{input_rate}->{output_rate}, {frequency:.3} Hz: gain {:.6} dB, residual rejection {:.2} dB, delay {delay_frames:.6} frames",
                    measurement.gain_db, measurement.residual_rejection_db
                ));
            }
        }
        eprintln!(
            "passband {input_rate}->{output_rate}: max error {maximum_gain_error:.6} dB, min residual rejection {minimum_residual_rejection:.2} dB, max delay {maximum_delay_error:.6} frames"
        );
    }
    eprintln!("passband sweep completed in {:?}", started.elapsed());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn fft_rate_pairs_preserve_absolute_passband_and_delay() {
    assert_passband(FFT_RATES);
}

#[test]
fn unusual_sinc_rate_pairs_preserve_absolute_passband_and_delay() {
    assert_passband(SINC_RATES);
}

#[test]
fn downsampling_rejects_tones_above_output_nyquist_by_eighty_db() {
    let started = Instant::now();
    let mut failures = Vec::new();
    for &(input_rate, output_rate) in ALL_RATES {
        if input_rate < output_rate {
            continue;
        }
        let output_nyquist = f64::from(output_rate) / 2.0;
        let input_nyquist = f64::from(input_rate) / 2.0;
        let mut minimum_rejection = f64::INFINITY;
        for frequency in [
            output_nyquist * 1.001,
            output_nyquist + (input_nyquist - output_nyquist) * 0.25,
            output_nyquist + (input_nyquist - output_nyquist) * 0.5,
            input_nyquist * 0.99,
        ] {
            let input = sine(input_rate, input_rate as usize, frequency);
            let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
            let measured_rms = rms(&output[4096..output.len() - 4096]);
            // Total residual energy detects all aliases, not just one presumed FFT bin.
            let rejection = -20.0 * (measured_rms / (SOURCE_AMPLITUDE / 2.0_f64.sqrt())).log10();
            minimum_rejection = minimum_rejection.min(rejection);
            if rejection < MIN_REJECTION_DB {
                failures.push(format!(
                    "{input_rate}->{output_rate}, {frequency:.3} Hz: rejection {rejection:.2} dB"
                ));
            }
        }
        eprintln!(
            "stopband {input_rate}->{output_rate}: min total alias rejection {minimum_rejection:.2} dB"
        );
    }
    eprintln!("stopband sweep completed in {:?}", started.elapsed());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn transition_rolls_off_toward_nyquist_without_an_unexpected_boost() {
    let started = Instant::now();
    for &(input_rate, output_rate) in ALL_RATES {
        let lower_nyquist = f64::from(input_rate.min(output_rate)) / 2.0;
        let mut gains = Vec::new();
        for fraction in [0.90, 0.95, 0.99] {
            let frequency = lower_nyquist * fraction;
            let input = sine(input_rate, input_rate as usize, frequency);
            let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
            let measurement = measure_tone(
                &output[4096..output.len() - 4096],
                4096,
                output_rate,
                frequency,
            );
            assert!(measurement.gain_db.is_finite());
            assert!(
                measurement.gain_db <= PASSBAND_TOLERANCE_DB,
                "unexpected transition boost: {input_rate}->{output_rate}, fraction {fraction}: {:.3} dB",
                measurement.gain_db
            );
            if fraction == 0.99 {
                assert!(
                    measurement.gain_db <= -3.0,
                    "filter must roll off before Nyquist: {input_rate}->{output_rate}: {:.3} dB",
                    measurement.gain_db
                );
            }
            gains.push(measurement.gain_db);
        }
        eprintln!(
            "transition {input_rate}->{output_rate}, 90/95/99% lower Nyquist: {:.3}/{:.3}/{:.3} dB",
            gains[0], gains[1], gains[2]
        );
    }
    eprintln!("transition sweep completed in {:?}", started.elapsed());
}

#[test]
fn impulses_keep_their_absolute_position_including_the_final_source_frame() {
    let started = Instant::now();
    for &(input_rate, output_rate) in ALL_RATES {
        let frames = 10_001;
        let mut maximum_position_error = 0.0_f64;
        for position in [0, 1023, 1024, 4095, frames - 1] {
            let mut source = vec![0.0; frames * 2];
            source[position * 2] = 0.5;
            let output = convert(input_rate, output_rate, 2, &source, IRREGULAR_CHUNKS);
            assert_eq!(
                output.len(),
                expected_frames(frames, input_rate, output_rate) * 2
            );
            let (output_frames, remainder) = output.as_chunks::<2>();
            assert!(remainder.is_empty());
            assert!(output_frames.iter().all(|frame| frame[1] == 0.0));
            let (peak_frame, peak) = output_frames
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| left[0].abs().total_cmp(&right[0].abs()))
                .expect("non-empty output");
            let expected_position =
                position as f64 * f64::from(output_rate) / f64::from(input_rate);
            let position_error = (peak_frame as f64 - expected_position).abs();
            maximum_position_error = maximum_position_error.max(position_error);
            // A sampled peak localizes a continuous impulse only to its nearest half-frame.
            // The passband phase test separately enforces the one-frame delay budget.
            assert!(
                position_error <= MAX_DELAY_ERROR_FRAMES + 0.5,
                "{input_rate}->{output_rate}, impulse {position}: peak {peak_frame}, expected {expected_position:.6}"
            );
            assert!(
                peak[0].abs() > 0.01,
                "final impulse must not be lost during draining: {input_rate}->{output_rate}, position {position}"
            );
            assert!(output.iter().all(|sample| sample.is_finite()));
        }
        eprintln!(
            "impulse {input_rate}->{output_rate}: max discrete position error {maximum_position_error:.6} frames"
        );
    }
    eprintln!(
        "impulse positions and tails completed in {:?}",
        started.elapsed()
    );
}

#[test]
fn silence_and_short_streams_have_exact_cumulative_ceil_frame_counts() {
    let started = Instant::now();
    for &(input_rate, output_rate) in ALL_RATES {
        for frames in [0, 1, 2, 7, 101, 1023, 1024, 1025, 10_001] {
            let source = vec![0.0; frames * 2];
            let output = convert(input_rate, output_rate, 2, &source, IRREGULAR_CHUNKS);
            assert_eq!(
                output.len(),
                expected_frames(frames, input_rate, output_rate) * 2,
                "{input_rate}->{output_rate}, {frames} source frames"
            );
            assert!(output.iter().all(|sample| *sample == 0.0));
        }
    }
    eprintln!(
        "silence and frame counts completed in {:?}",
        started.elapsed()
    );
}

#[test]
fn separated_tone_bursts_retain_timing_and_the_last_burst_after_eof() {
    let started = Instant::now();
    for &(input_rate, output_rate) in ALL_RATES {
        let frames = input_rate as usize / 2;
        let frequency = 1_000.0;
        let mut source = sine(input_rate, frames, frequency);
        // Deliberately sharp gates exercise ringing and an active final filter tail.
        let starts = [input_rate as usize / 10, input_rate as usize * 3 / 10];
        let ends = [input_rate as usize / 5, frames];
        for (frame, sample) in source.iter_mut().enumerate() {
            if !(starts[0]..ends[0]).contains(&frame) && !(starts[1]..ends[1]).contains(&frame) {
                *sample = 0.0;
            }
        }
        let output = convert(input_rate, output_rate, 1, &source, &[1, 101, 1023, 197]);
        assert_eq!(
            output.len(),
            expected_frames(frames, input_rate, output_rate)
        );
        for (start, end) in starts.into_iter().zip(ends) {
            let first = expected_frames(start, input_rate, output_rate) + 1024;
            let last = expected_frames(end, input_rate, output_rate) - 1024;
            let measurement = measure_tone(&output[first..last], first, output_rate, frequency);
            assert!(measurement.gain_db.abs() <= PASSBAND_TOLERANCE_DB);
            let delay = measurement.phase_error.abs() / (TAU * frequency / f64::from(output_rate));
            assert!(delay <= MAX_DELAY_ERROR_FRAMES);
            assert!(measurement.residual_rejection_db >= MIN_REJECTION_DB);
        }
        let gap_start = expected_frames(ends[0], input_rate, output_rate) + 1024;
        let gap_end = expected_frames(starts[1], input_rate, output_rate) - 1024;
        assert!(
            output[gap_start..gap_end]
                .iter()
                .all(|sample| *sample == 0.0)
        );
        assert!(
            rms(&output[output.len() - 101..]) > 0.2,
            "EOF must contain the final burst rather than an undrained delay"
        );
    }
    eprintln!("burst timing and EOF completed in {:?}", started.elapsed());
}

#[test]
fn extreme_rate_ratios_preserve_quality_with_large_internal_input_chunks() {
    let started = Instant::now();
    for (input_rate, output_rate) in [(384_000_u32, 8_000_u32), (8_000, 384_000)] {
        let source_frames = input_rate as usize * 2;
        let lower_rate = input_rate.min(output_rate);
        // Upsampling stretches filter support in output frames; exclude the full support.
        let trim = 4096.max(expected_frames(256, lower_rate, output_rate));
        let mut maximum_gain_error = 0.0_f64;
        let mut minimum_residual_rejection = f64::INFINITY;
        for frequency in [20.0, 1_000.0, 3_200.0] {
            let input = sine(input_rate, source_frames, frequency);
            let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
            assert_eq!(
                output.len(),
                expected_frames(source_frames, input_rate, output_rate)
            );
            assert!(output.iter().all(|sample| sample.is_finite()));
            let measurement = measure_tone(
                &output[trim..output.len() - trim],
                trim,
                output_rate,
                frequency,
            );
            maximum_gain_error = maximum_gain_error.max(measurement.gain_db.abs());
            minimum_residual_rejection =
                minimum_residual_rejection.min(measurement.residual_rejection_db);
            assert!(
                measurement.gain_db.abs() <= PASSBAND_TOLERANCE_DB,
                "{input_rate}->{output_rate}, {frequency} Hz: gain {:.6} dB",
                measurement.gain_db
            );
            assert!(
                measurement.residual_rejection_db >= MIN_REJECTION_DB,
                "{input_rate}->{output_rate}, {frequency} Hz: residual rejection {:.2} dB",
                measurement.residual_rejection_db
            );
        }
        eprintln!(
            "extreme passband {input_rate}->{output_rate}: max error {maximum_gain_error:.6} dB, min residual rejection {minimum_residual_rejection:.2} dB"
        );
        if input_rate > output_rate {
            let mut minimum_rejection = f64::INFINITY;
            for frequency in [4_001.0, 4_800.0, 12_000.0, 190_000.0] {
                let input = sine(input_rate, source_frames, frequency);
                let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
                assert_eq!(
                    output.len(),
                    expected_frames(source_frames, input_rate, output_rate)
                );
                assert!(output.iter().all(|sample| sample.is_finite()));
                let rejection = -20.0
                    * (rms(&output[trim..output.len() - trim])
                        / (SOURCE_AMPLITUDE / 2.0_f64.sqrt()))
                    .log10();
                minimum_rejection = minimum_rejection.min(rejection);
                assert!(
                    rejection >= MIN_REJECTION_DB,
                    "{input_rate}->{output_rate}, {frequency} Hz: rejection {rejection:.2} dB"
                );
            }
            eprintln!(
                "extreme stopband {input_rate}->{output_rate}: min total alias rejection {minimum_rejection:.2} dB"
            );
        }
    }
    eprintln!("extreme ratio quality completed in {:?}", started.elapsed());
}

#[test]
fn unusual_large_sinc_downsampling_ratios_preserve_passband_and_reject_aliases() {
    let started = Instant::now();
    let mut failures = Vec::new();
    for (input_rate, output_rate) in [
        (96_000_u32, 44_101_u32),
        (192_000, 48_001),
        (384_000, 8_001),
    ] {
        let source_frames = input_rate as usize * 2;
        let output_nyquist = f64::from(output_rate) / 2.0;
        let input_nyquist = f64::from(input_rate) / 2.0;
        let expected = expected_frames(source_frames, input_rate, output_rate);
        let trim = 4096;
        for frequency in [20.0, 1_000.0, output_nyquist * 0.8] {
            let input = sine(input_rate, source_frames, frequency);
            let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
            assert_eq!(output.len(), expected);
            assert!(output.iter().all(|sample| sample.is_finite()));
            let measurement = measure_tone(
                &output[trim..output.len() - trim],
                trim,
                output_rate,
                frequency,
            );
            eprintln!(
                "large sinc passband {input_rate}->{output_rate}, {frequency:.3} Hz: gain {:.6} dB, residual rejection {:.2} dB",
                measurement.gain_db, measurement.residual_rejection_db
            );
            if measurement.gain_db.abs() > PASSBAND_TOLERANCE_DB
                || measurement.residual_rejection_db < MIN_REJECTION_DB
            {
                failures.push(format!(
                    "{input_rate}->{output_rate}, {frequency:.3} Hz: gain {:.6} dB, residual rejection {:.2} dB",
                    measurement.gain_db, measurement.residual_rejection_db
                ));
            }
        }
        for frequency in [
            output_nyquist * 1.001,
            input_nyquist * 0.5,
            input_nyquist * 0.99,
        ] {
            let input = sine(input_rate, source_frames, frequency);
            let output = convert(input_rate, output_rate, 1, &input, IRREGULAR_CHUNKS);
            assert_eq!(output.len(), expected);
            assert!(output.iter().all(|sample| sample.is_finite()));
            let rejection = -20.0
                * (rms(&output[trim..output.len() - trim]) / (SOURCE_AMPLITUDE / 2.0_f64.sqrt()))
                    .log10();
            eprintln!(
                "large sinc stopband {input_rate}->{output_rate}, {frequency:.3} Hz: total alias rejection {rejection:.2} dB"
            );
            if rejection < MIN_REJECTION_DB {
                failures.push(format!(
                    "{input_rate}->{output_rate}, {frequency:.3} Hz: total alias rejection {rejection:.2} dB"
                ));
            }
        }
    }
    eprintln!(
        "large sinc ratio quality completed in {:?}",
        started.elapsed()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
