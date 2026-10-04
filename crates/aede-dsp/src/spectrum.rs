//! Frequency-band levels for a playback visualizer.

use std::f32::consts::TAU;

use crate::{DspError, PcmFormat};

/// Number of logarithmic display bands between 45 Hz and min(Nyquist, 16 kHz).
pub const SPECTRUM_BANDS: usize = 24;
const WINDOW: usize = 2048;

/// Twenty-four smoothed frequency-band levels from interleaved PCM.
///
/// The values are display levels in `0..=1`, not loudness measurements. Each
/// completed window uses a Hann taper and a real-input FFT. The source PCM is
/// only read; playback sends the same samples to the output device.
pub struct Spectrum {
    format: PcmFormat,
    samples: [f32; WINDOW],
    taper: [f32; WINDOW],
    filled: usize,
    levels: [f32; SPECTRUM_BANDS],
}

impl Spectrum {
    /// Prepare a fresh fixed-format 2048-frame analysis window.
    /// Build this outside the real-time audio callback.
    pub fn new(format: PcmFormat) -> Self {
        let mut taper = [0.0; WINDOW];
        for (index, weight) in taper.iter_mut().enumerate() {
            *weight = 0.5 * (1.0 - (TAU * index as f32 / (WINDOW - 1) as f32).cos());
        }
        Self {
            format,
            samples: [0.0; WINDOW],
            taper,
            filled: 0,
            levels: [0.0; SPECTRUM_BANDS],
        }
    }

    /// Consume complete PCM frames and return the latest completed window.
    ///
    /// Incomplete or non-finite input leaves both smoothing and partially
    /// collected PCM unchanged. A block may complete multiple windows; the
    /// result describes the last one, or `None` if no window completed.
    pub fn push(&mut self, samples: &[f32]) -> Result<Option<[f32; SPECTRUM_BANDS]>, DspError> {
        let channels = usize::from(self.format.channels());
        if !samples.len().is_multiple_of(channels) {
            return Err(DspError::IncompleteFrame);
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(DspError::NonFiniteSample);
        }
        let mut latest = None;
        for frame in samples.chunks_exact(channels) {
            // Giving the first channel a larger weight keeps opposite-phase
            // stereo from cancelling into silence in this display mix.
            self.samples[self.filled] = if channels == 1 {
                frame[0]
            } else {
                let others = frame[1..].iter().copied().sum::<f32>() / (channels - 1) as f32;
                frame[0] * 0.75 + others * 0.25
            };
            self.filled += 1;
            if self.filled == WINDOW {
                self.analyze();
                self.filled = 0;
                latest = Some(self.levels);
            }
        }
        Ok(latest)
    }

    fn analyze(&mut self) {
        let mut real = [0.0; WINDOW];
        let mut imaginary = [0.0; WINDOW];
        for ((out, sample), taper) in real.iter_mut().zip(&self.samples).zip(&self.taper) {
            *out = sample * taper;
        }
        fft(&mut real, &mut imaginary);

        let high = (self.format.sample_rate() as f32 / 2.0).min(16_000.0);
        let mut peaks = [0.0_f32; SPECTRUM_BANDS];
        if high > 45.0 {
            let logarithmic_width = (high / 45.0).ln() / SPECTRUM_BANDS as f32;
            for index in 1..WINDOW / 2 {
                let hz = index as f32 * self.format.sample_rate() as f32 / WINDOW as f32;
                if !(45.0..=high).contains(&hz) {
                    continue;
                }
                let band =
                    (((hz / 45.0).ln() / logarithmic_width) as usize).min(SPECTRUM_BANDS - 1);
                let amplitude = real[index].hypot(imaginary[index]) * 4.0 / WINDOW as f32;
                peaks[band] = peaks[band].max(amplitude);
            }
        }
        for (level, peak) in self.levels.iter_mut().zip(peaks) {
            let target = if peak > 0.0 {
                ((20.0 * peak.log10() + 60.0) / 50.0).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let retained = if target > *level { 0.25 } else { 0.70 };
            *level = *level * retained + target * (1.0 - retained);
        }
    }
}

fn fft(real: &mut [f32; WINDOW], imaginary: &mut [f32; WINDOW]) {
    let mut swapped = 0;
    for index in 1..WINDOW {
        let mut bit = WINDOW / 2;
        while swapped & bit != 0 {
            swapped ^= bit;
            bit /= 2;
        }
        swapped ^= bit;
        if index < swapped {
            real.swap(index, swapped);
            imaginary.swap(index, swapped);
        }
    }

    let mut span = 2;
    while span <= WINDOW {
        let phase = -TAU / span as f32;
        let (step_imaginary, step_real) = phase.sin_cos();
        for start in (0..WINDOW).step_by(span) {
            let (mut rotation_real, mut rotation_imaginary) = (1.0, 0.0);
            for offset in 0..span / 2 {
                let left = start + offset;
                let right = left + span / 2;
                let right_real =
                    real[right] * rotation_real - imaginary[right] * rotation_imaginary;
                let right_imaginary =
                    real[right] * rotation_imaginary + imaginary[right] * rotation_real;
                real[right] = real[left] - right_real;
                imaginary[right] = imaginary[left] - right_imaginary;
                real[left] += right_real;
                imaginary[left] += right_imaginary;
                let next_real = rotation_real * step_real - rotation_imaginary * step_imaginary;
                rotation_imaginary =
                    rotation_real * step_imaginary + rotation_imaginary * step_real;
                rotation_real = next_real;
            }
        }
        span *= 2;
    }
}

#[cfg(test)]
#[path = "spectrum_tests.rs"]
mod tests;
