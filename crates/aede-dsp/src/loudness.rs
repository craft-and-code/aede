//! Gated programme loudness and true peak measurements over decoded PCM.
//!
//! This layer knows only sample format and samples. Decoding, file identity,
//! metadata precedence, and persistence belong to the playback core.

use std::fmt;

use ebur128::{EbuR128, Mode};

use crate::PcmFormat;
use crate::peak_tail::PeakTail;

/// Integrated programme loudness and peak of the original PCM.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    /// Gated programme loudness in LUFS.
    pub integrated_lufs: f32,
    /// Linear true peak, when every programme section has an oversampled
    /// estimate; may exceed one. Unavailable at 192 kHz and above.
    pub true_peak: Option<f32>,
}

/// Invalid sample layout or a measurement-library failure.
#[derive(Debug)]
pub enum LoudnessError {
    /// Speaker positions are unknown for more than two channels.
    UnsupportedLayout,
    /// An input block is not a complete set of finite frames.
    InvalidBuffer,
    /// The underlying EBU R128 meter refused the operation.
    Meter(ebur128::Error),
}

impl fmt::Display for LoudnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedLayout => f.write_str("loudness requires mono or stereo PCM"),
            Self::InvalidBuffer => f.write_str("loudness requires complete finite PCM frames"),
            Self::Meter(error) => write!(f, "loudness meter: {error}"),
        }
    }
}

impl std::error::Error for LoudnessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Meter(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ebur128::Error> for LoudnessError {
    fn from(error: ebur128::Error) -> Self {
        Self::Meter(error)
    }
}

/// Streaming meter for an ordered programme, including format changes.
///
/// Consecutive blocks of the same format share one meter. At a format change,
/// a new meter starts and the final gated result combines both histories.
/// This avoids averaging per-track LUFS; a format boundary does not contribute
/// windows that cross that boundary.
#[derive(Default)]
pub struct LoudnessProgramme {
    sections: Vec<Section>,
}

struct Section {
    meter: EbuR128,
    peak_tail: PeakTail,
}

impl LoudnessProgramme {
    /// Create an empty programme meter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add finite, interleaved PCM without changing the caller's samples.
    pub fn push(&mut self, format: PcmFormat, samples: &[f32]) -> Result<(), LoudnessError> {
        let channels = usize::from(format.channels());
        if !matches!(format.channels(), 1 | 2) {
            return Err(LoudnessError::UnsupportedLayout);
        }
        if samples.is_empty()
            || !samples.len().is_multiple_of(channels)
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err(LoudnessError::InvalidBuffer);
        }
        let reuse = self.sections.last().is_some_and(|section| {
            section.meter.rate() == format.sample_rate()
                && section.meter.channels() == u32::from(format.channels())
        });
        if !reuse {
            self.sections.push(Section {
                meter: EbuR128::new(
                    u32::from(format.channels()),
                    format.sample_rate(),
                    Mode::I | Mode::TRUE_PEAK,
                )?,
                peak_tail: PeakTail::new(format),
            });
        }
        if let Some(section) = self.sections.last_mut() {
            section.meter.add_frames_f32(samples)?;
            section.peak_tail.push(samples);
        }
        Ok(())
    }

    /// Measure the finite programme; silence and streams under one gate have no LUFS.
    ///
    /// Resolve delayed true-peak outputs with a separate zero-extended tail.
    /// This does not add silent loudness frames or modify subsequent capture.
    pub fn measurement(&self) -> Result<Option<Measurement>, LoudnessError> {
        if self.sections.is_empty() {
            return Ok(None);
        }
        let loudness =
            EbuR128::loudness_global_multiple(self.sections.iter().map(|section| &section.meter))?;
        if !loudness.is_finite() || !(-100.0..=20.0).contains(&loudness) {
            return Ok(None);
        }
        // ebur128 does not oversample at 192 kHz or above. A sample peak
        // from even one section cannot establish the programme's true peak.
        let true_peak = if self
            .sections
            .iter()
            .all(|section| section.meter.rate() < 192_000)
        {
            let mut peak = 0.0f32;
            for section in &self.sections {
                if let Some(section_peak) = section.peak_tail.peak(&section.meter)? {
                    peak = peak.max(section_peak);
                }
            }
            if !peak.is_finite() {
                return Ok(None);
            }
            Some(peak)
        } else {
            None
        };
        Ok(Some(Measurement {
            integrated_lufs: loudness as f32,
            true_peak,
        }))
    }
}

#[cfg(test)]
#[path = "loudness_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "loudness_reference_tests.rs"]
mod reference_tests;

#[cfg(test)]
#[path = "loudness_tail_tests.rs"]
mod tail_tests;
