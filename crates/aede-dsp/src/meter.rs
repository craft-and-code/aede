//! Measurements of processed PCM submitted to an output sink.

use std::fmt;

use ebur128::{EbuR128, Mode};

use crate::peak_tail::PeakTail;
use crate::{PcmFormat, ProcessStats};

#[derive(Debug)]
pub enum OutputMeterError {
    InvalidBuffer,
    Meter(ebur128::Error),
}

impl fmt::Display for OutputMeterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBuffer => f.write_str("output meter needs complete, guarded PCM frames"),
            Self::Meter(error) => write!(f, "true-peak meter failed: {error}"),
        }
    }
}

impl std::error::Error for OutputMeterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Meter(error) => Some(error),
            Self::InvalidBuffer => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputSnapshot {
    /// Complete PCM frames submitted to the sink.
    pub frames: u64,
    /// Highest sample after gain/EQ but before the hard output guard.
    pub pre_guard_sample_peak: f32,
    /// Highest sample actually submitted to the sink, before any dither.
    pub output_sample_peak: f32,
    /// Estimated inter-sample peak before any dither/device conversion.
    /// Unavailable when this meter cannot provide an oversampled estimate.
    pub output_true_peak: Option<f32>,
    /// Samples changed by the hard full-scale guard.
    pub guarded_samples: u64,
}

/// A per-track meter over the signal being submitted to an output sink.
///
/// The true-peak estimate uses ebur128's oversampling below 192 kHz. At
/// 192 kHz and above that library does not oversample, so `output_true_peak`
/// is deliberately unavailable rather than relabeling sample peak as true peak.
pub struct OutputMeter {
    format: PcmFormat,
    true_peak: Option<EbuR128>,
    peak_tail: PeakTail,
    frames: u64,
    pre_guard_sample_peak: f32,
    output_sample_peak: f32,
    guarded_samples: u64,
}

impl OutputMeter {
    /// Prepare sample/guard diagnostics and oversampled true peak when supported.
    /// True peak is unavailable at sample rates of 192 kHz and above.
    pub fn new(format: PcmFormat) -> Result<Self, OutputMeterError> {
        let true_peak = if format.sample_rate() < 192_000 {
            Some(
                EbuR128::new(
                    u32::from(format.channels()),
                    format.sample_rate(),
                    Mode::TRUE_PEAK,
                )
                .map_err(OutputMeterError::Meter)?,
            )
        } else {
            None
        };
        Ok(Self {
            format,
            true_peak,
            peak_tail: PeakTail::new(format),
            frames: 0,
            pre_guard_sample_peak: 0.0,
            output_sample_peak: 0.0,
            guarded_samples: 0,
        })
    }

    /// Keep sample-peak and guard diagnostics if true-peak metering cannot start.
    pub fn sample_peak_only(format: PcmFormat) -> Self {
        Self {
            format,
            true_peak: None,
            peak_tail: PeakTail::new(format),
            frames: 0,
            pre_guard_sample_peak: 0.0,
            output_sample_peak: 0.0,
            guarded_samples: 0,
        }
    }

    /// Observe one guarded block with its matching pre-guard processing stats.
    ///
    /// Invalid frames or inconsistent sample peaks are refused before counters
    /// change. Meter-library failure retains sample/guard diagnostics and
    /// disables true-peak capture; the caller must report that error.
    pub fn observe(
        &mut self,
        guarded_samples: &[f32],
        stats: ProcessStats,
    ) -> Result<(), OutputMeterError> {
        let channels = usize::from(self.format.channels());
        if guarded_samples.is_empty()
            || !guarded_samples.len().is_multiple_of(channels)
            || guarded_samples
                .iter()
                .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
            || !stats.sample_peak.is_finite()
            || stats.overfull_samples > guarded_samples.len()
        {
            return Err(OutputMeterError::InvalidBuffer);
        }
        let sample_peak = guarded_samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        if stats.sample_peak < sample_peak {
            return Err(OutputMeterError::InvalidBuffer);
        }
        self.frames = self
            .frames
            .saturating_add((guarded_samples.len() / channels) as u64);
        self.pre_guard_sample_peak = self.pre_guard_sample_peak.max(stats.sample_peak);
        self.output_sample_peak = self.output_sample_peak.max(sample_peak);
        self.guarded_samples = self
            .guarded_samples
            .saturating_add(stats.overfull_samples as u64);
        if let Some(meter) = &mut self.true_peak {
            if let Err(error) = meter.add_frames_f32(guarded_samples) {
                self.true_peak = None;
                return Err(OutputMeterError::Meter(error));
            }
            self.peak_tail.push(guarded_samples);
        }
        Ok(())
    }

    /// Treat the submitted prefix as a finite signal, including its FIR tail.
    /// No silent frames are submitted or counted, and capture can continue.
    pub fn snapshot(&self) -> Result<OutputSnapshot, OutputMeterError> {
        let mut snapshot = self.sample_peak_snapshot();
        if let Some(meter) = &self.true_peak
            && self.frames > 0
        {
            let peak = self
                .peak_tail
                .peak(meter)
                .map_err(OutputMeterError::Meter)?;
            if peak.is_some_and(|value| !value.is_finite()) {
                return Err(OutputMeterError::InvalidBuffer);
            }
            snapshot.output_true_peak = peak;
        }
        Ok(snapshot)
    }

    /// Preserve the sample and guard measurements if true-peak reporting fails.
    pub fn sample_peak_snapshot(&self) -> OutputSnapshot {
        OutputSnapshot {
            frames: self.frames,
            pre_guard_sample_peak: self.pre_guard_sample_peak,
            output_sample_peak: self.output_sample_peak,
            output_true_peak: None,
            guarded_samples: self.guarded_samples,
        }
    }
}

#[cfg(test)]
#[path = "meter_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "meter_reference_tests.rs"]
mod reference_tests;
