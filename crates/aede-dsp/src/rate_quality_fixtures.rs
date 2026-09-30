use super::{PcmFormat, RateConverter};
use std::f64::consts::TAU;

pub(super) const FFT_RATES: &[(u32, u32)] = &[
    (44_100, 48_000),
    (48_000, 44_100),
    (96_000, 44_100),
    (192_000, 48_000),
];
pub(super) const SINC_RATES: &[(u32, u32)] = &[(44_101, 48_000), (48_000, 44_101)];
pub(super) const ALL_RATES: &[(u32, u32)] = &[
    (44_100, 48_000),
    (48_000, 44_100),
    (96_000, 44_100),
    (192_000, 48_000),
    (44_101, 48_000),
    (48_000, 44_101),
];
pub(super) const SOURCE_AMPLITUDE: f64 = 0.5;
pub(super) const SOURCE_PHASE: f64 = 0.347;

pub(super) fn sine(rate: u32, frames: usize, frequency: f64) -> Vec<f32> {
    (0..frames)
        .map(|frame| {
            (SOURCE_AMPLITUDE
                * (TAU * frequency * frame as f64 / f64::from(rate) + SOURCE_PHASE).sin())
                as f32
        })
        .collect()
}

pub(super) fn convert(
    input_rate: u32,
    output_rate: u32,
    channels: u16,
    source: &[f32],
    chunk_frames: &[usize],
) -> Vec<f32> {
    let channels_count = usize::from(channels);
    let mut converter = RateConverter::new(
        PcmFormat::new(input_rate, channels).expect("test PCM format"),
        output_rate,
    )
    .expect("test rate pair");
    let mut output = Vec::new();
    let mut position = 0;
    let mut chunk_index = 0;
    while position < source.len() {
        let requested = chunk_frames[chunk_index % chunk_frames.len()] * channels_count;
        let end = (position + requested).min(source.len());
        output.extend_from_slice(
            converter
                .push(&source[position..end], false)
                .expect("source block"),
        );
        position = end;
        chunk_index += 1;
    }
    output.extend_from_slice(converter.push(&[], true).expect("EOF tail"));
    output
}

pub(super) fn expected_frames(source_frames: usize, input_rate: u32, output_rate: u32) -> usize {
    ((source_frames as u64 * u64::from(output_rate)).div_ceil(u64::from(input_rate))) as usize
}

pub(super) fn rms(samples: &[f32]) -> f64 {
    (samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt()
}

pub(super) struct ToneMeasurement {
    pub(super) gain_db: f64,
    pub(super) residual_rejection_db: f64,
    pub(super) phase_error: f64,
}

// Least-squares sin/cos projection avoids FFT-bin alignment and leakage assumptions.
// Its phase is referenced to the analytical source timeline, including the trimmed prefix.
pub(super) fn measure_tone(
    samples: &[f32],
    first_frame: usize,
    rate: u32,
    frequency: f64,
) -> ToneMeasurement {
    let angular_step = TAU * frequency / f64::from(rate);
    let mut sin_sin = 0.0;
    let mut cos_cos = 0.0;
    let mut sin_cos = 0.0;
    let mut sample_sin = 0.0;
    let mut sample_cos = 0.0;
    for (frame, sample) in samples.iter().enumerate() {
        let (sin, cos) = (angular_step * (first_frame + frame) as f64).sin_cos();
        sin_sin += sin * sin;
        cos_cos += cos * cos;
        sin_cos += sin * cos;
        sample_sin += f64::from(*sample) * sin;
        sample_cos += f64::from(*sample) * cos;
    }
    let determinant = sin_sin * cos_cos - sin_cos * sin_cos;
    assert!(determinant > 0.0, "independent sin/cos references");
    let sin_gain = (sample_sin * cos_cos - sample_cos * sin_cos) / determinant;
    let cos_gain = (sample_cos * sin_sin - sample_sin * sin_cos) / determinant;
    let amplitude = sin_gain.hypot(cos_gain);
    let residual_rms = (samples
        .iter()
        .enumerate()
        .map(|(frame, sample)| {
            let (sin, cos) = (angular_step * (first_frame + frame) as f64).sin_cos();
            (f64::from(*sample) - sin_gain * sin - cos_gain * cos).powi(2)
        })
        .sum::<f64>()
        / samples.len() as f64)
        .sqrt();
    let phase = cos_gain.atan2(sin_gain) - SOURCE_PHASE;
    ToneMeasurement {
        gain_db: 20.0 * (amplitude / SOURCE_AMPLITUDE).log10(),
        residual_rejection_db: -20.0 * (residual_rms / (SOURCE_AMPLITUDE / 2.0_f64.sqrt())).log10(),
        phase_error: (phase + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI,
    }
}
