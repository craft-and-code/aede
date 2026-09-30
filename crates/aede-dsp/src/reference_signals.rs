//! Independently synthesized mathematical signals for offline DSP tests.
//!
//! These helpers do not contain or derive samples from the restricted EBU WAV
//! archive. Test definitions can cite EBU Tech 3341's mathematical parameters:
//! https://tech.ebu.ch/docs/tech/tech3341.pdf

pub(crate) fn amplitude(dbfs: f64) -> f32 {
    10.0_f64.powf(dbfs / 20.0) as f32
}

/// Generate mono PCM with phase in degrees and frequency in Hz.
pub(crate) fn sine(
    rate: u32,
    frames: usize,
    amplitude: f32,
    frequency: f64,
    phase_degrees: f64,
) -> Vec<f32> {
    let phase = phase_degrees.to_radians();
    (0..frames)
        .map(|frame| {
            (f64::from(amplitude)
                * (std::f64::consts::TAU * frequency * frame as f64 / f64::from(rate) + phase)
                    .sin()) as f32
        })
        .collect()
}

/// Duplicate a mono signal into in-phase, interleaved left and right channels.
pub(crate) fn stereo(mono: &[f32]) -> Vec<f32> {
    mono.iter().flat_map(|sample| [*sample, *sample]).collect()
}

/// Generate in-phase stereo 1 kHz steps: `(seconds, per-channel peak dBFS)`.
///
/// Phase is continuous across steps. Each duration is rounded to whole frames.
pub(crate) fn stepped_stereo(rate: u32, steps: &[(f64, f64)]) -> Vec<f32> {
    let frames = |seconds: f64| (seconds * f64::from(rate)).round() as usize;
    let total_frames: usize = steps.iter().map(|(seconds, _)| frames(*seconds)).sum();
    let mut samples = Vec::with_capacity(total_frames * 2);
    let mut frame = 0;
    for (seconds, dbfs) in steps {
        let level = f64::from(amplitude(*dbfs));
        for _ in 0..frames(*seconds) {
            let sample = (level
                * (std::f64::consts::TAU * 1000.0 * frame as f64 / f64::from(rate)).sin())
                as f32;
            samples.extend_from_slice(&[sample, sample]);
            frame += 1;
        }
    }
    samples
}
