//! Sample processing between a decoder and an audio output.
//!
//! The decoder supplies finite, interleaved `f32` PCM. `Dsp` processes
//! complete frames in place without allocating. The sample rate and channel
//! count stay fixed for the lifetime of a [`Dsp`]; a format change requires a
//! new instance. Values above full scale remain available to later DSP stages.
//! The output adapter must decide how to handle them when converting to its
//! device format.

use std::fmt;

mod spectrum;
pub use spectrum::{SPECTRUM_BANDS, Spectrum};
pub mod loudness;

/// The decoded PCM layout used by a processing stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmFormat {
    sample_rate: u32,
    channels: u16,
}

impl PcmFormat {
    /// Rejects rates and channel counts that cannot describe audio frames.
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self, DspError> {
        if sample_rate == 0 || channels == 0 {
            return Err(DspError::InvalidFormat);
        }
        Ok(Self {
            sample_rate,
            channels,
        })
    }

    pub fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    pub fn channels(self) -> u16 {
        self.channels
    }
}

/// Input errors leave the audio buffer and processor state unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DspError {
    InvalidFormat,
    InvalidGain,
    InvalidPeak,
    IncompleteFrame,
    NonFiniteSample,
    SampleOverflow,
}

impl fmt::Display for DspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidFormat => "PCM sample rate and channel count must be positive",
            Self::InvalidGain => "gain in dB must produce a finite, positive multiplier",
            Self::InvalidPeak => "source peak must be finite and non-negative",
            Self::IncompleteFrame => "PCM buffer does not contain complete frames",
            Self::NonFiniteSample => "PCM buffer contains a non-finite sample",
            Self::SampleOverflow => "gain would overflow an f32 sample",
        };
        f.write_str(message)
    }
}

impl std::error::Error for DspError {}

/// Cap a requested gain so the declared source sample peak cannot exceed full scale.
///
/// When no peak is known, the source is conservatively assumed to reach 1.0.
/// This cannot catch inaccurate metadata or inter-sample peaks; call
/// [`protect_output`] after all DSP stages before sending PCM to a sink.
pub fn gain_with_headroom_db(
    requested_gain_db: f32,
    source_peak: Option<f32>,
) -> Result<f32, DspError> {
    if !requested_gain_db.is_finite() {
        return Err(DspError::InvalidGain);
    }
    let peak = source_peak.unwrap_or(1.0);
    if !peak.is_finite() || peak < 0.0 {
        return Err(DspError::InvalidPeak);
    }
    let requested_scale = 10.0_f64.powf(f64::from(requested_gain_db) / 20.0);
    if !requested_scale.is_finite()
        || requested_scale <= 0.0
        || requested_scale > f64::from(f32::MAX)
    {
        return Err(DspError::InvalidGain);
    }
    if peak == 0.0 || f64::from(peak) * requested_scale <= 1.0 {
        return Ok(requested_gain_db);
    }
    Ok((20.0 * (1.0 / f64::from(peak)).log10()) as f32)
}

/// Apply the final sample-peak safety ceiling before any output format.
///
/// Returns the number of hard-clamped samples, so the caller can report any
/// audible intervention. In-range samples are untouched. An invalid buffer is
/// rejected without partial mutation. This does not guarantee true-peak safety.
pub fn protect_output(samples: &mut [f32]) -> Result<usize, DspError> {
    if samples.iter().any(|sample| !sample.is_finite()) {
        return Err(DspError::NonFiniteSample);
    }
    let mut clamped = 0;
    for sample in samples {
        if *sample > 1.0 {
            *sample = 1.0;
            clamped += 1;
        } else if *sample < -1.0 {
            *sample = -1.0;
            clamped += 1;
        }
    }
    Ok(clamped)
}

/// Measurements of the processed block, before the final output guard.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProcessStats {
    /// Highest absolute sample, including values above full scale.
    pub sample_peak: f32,
    /// Number of samples whose absolute value exceeds 1.0.
    pub overfull_samples: usize,
}

/// A continuous gain stage for decoded PCM.
///
/// Gain is shared by all channels of a frame. A requested change can be
/// spread over a number of frames so it continues smoothly across block
/// boundaries. This stage does not set the system volume or limit peaks.
#[derive(Clone, Debug)]
pub struct Dsp {
    format: PcmFormat,
    current_gain: f64,
    target_gain: f64,
    ramp_remaining: u64,
}

impl Dsp {
    /// Starts at unity gain.
    pub fn new(format: PcmFormat) -> Self {
        Self {
            format,
            current_gain: 1.0,
            target_gain: 1.0,
            ramp_remaining: 0,
        }
    }

    pub fn format(&self) -> PcmFormat {
        self.format
    }

    /// Set a stream gain, typically chosen from loudness metadata.
    ///
    /// `ramp_frames = 0` applies it immediately. Otherwise the first processed
    /// frame takes the first ramp step, and frame `ramp_frames` reaches the
    /// exact target. A later call replaces the remaining ramp from its current
    /// level. No output is clipped or silently limited.
    pub fn set_gain_db(&mut self, gain_db: f32, ramp_frames: u64) -> Result<(), DspError> {
        if !gain_db.is_finite() {
            return Err(DspError::InvalidGain);
        }
        let gain = 10.0_f64.powf(f64::from(gain_db) / 20.0);
        if !gain.is_finite() || gain <= 0.0 || gain > f64::from(f32::MAX) {
            return Err(DspError::InvalidGain);
        }
        self.target_gain = gain;
        self.ramp_remaining = ramp_frames;
        if ramp_frames == 0 {
            self.current_gain = gain;
        }
        Ok(())
    }

    /// Process complete interleaved frames in place, with no allocation.
    ///
    /// The whole buffer is checked before mutation, so a rejected block does
    /// not advance an in-progress gain ramp. Samples above 1.0 are measured,
    /// not clipped; the output adapter owns any final limiting or conversion.
    pub fn process(&mut self, samples: &mut [f32]) -> Result<ProcessStats, DspError> {
        let channels = usize::from(self.format.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(DspError::IncompleteFrame);
        }
        let maximum_gain = self.current_gain.max(self.target_gain);
        for &sample in samples.iter() {
            if !sample.is_finite() {
                return Err(DspError::NonFiniteSample);
            }
            if f64::from(sample.abs()) * maximum_gain > f64::from(f32::MAX) {
                return Err(DspError::SampleOverflow);
            }
        }

        let mut stats = ProcessStats::default();
        for frame in samples.chunks_exact_mut(channels) {
            if self.ramp_remaining > 0 {
                self.current_gain +=
                    (self.target_gain - self.current_gain) / self.ramp_remaining as f64;
                self.ramp_remaining -= 1;
                if self.ramp_remaining == 0 {
                    self.current_gain = self.target_gain;
                }
            }
            for sample in frame {
                *sample = (f64::from(*sample) * self.current_gain) as f32;
                let magnitude = sample.abs();
                stats.sample_peak = stats.sample_peak.max(magnitude);
                stats.overfull_samples += usize::from(magnitude > 1.0);
            }
        }
        Ok(stats)
    }

    /// Apply gain and the final sample-peak guard for an output stream.
    ///
    /// Statistics describe the signal before the guard. The returned buffer
    /// is ready for a full-scale floating PCM sink; any hard-clamped samples
    /// are counted by `overfull_samples` for a visible warning.
    pub fn process_for_output(&mut self, samples: &mut [f32]) -> Result<ProcessStats, DspError> {
        let stats = self.process(samples)?;
        protect_output(samples)?;
        Ok(stats)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
