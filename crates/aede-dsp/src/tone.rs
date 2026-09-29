//! Broad, optional bass and treble shelves for decoded PCM.

use biquad::{Biquad, Coefficients, DirectForm2Transposed, Q_BUTTERWORTH_F64, Type};

use crate::{DspError, PcmFormat};

/// Fixed shelf midpoints at ordinary sample rates. At lower rates each
/// midpoint stays below Nyquist so the filter remains well-defined.
const BASS_HZ: f64 = 120.0;
const TREBLE_HZ: f64 = 4_000.0;
const MAX_CORNER_FRACTION: f64 = 0.35;

/// Optional broad tone controls, each limited to ±12 dB.
///
/// Flat controls bypass both filters exactly. Replacing controls starts fresh
/// filter state and should be done between streams, not mid-block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneControls {
    bass_db: f32,
    treble_db: f32,
}

impl ToneControls {
    pub const FLAT: Self = Self {
        bass_db: 0.0,
        treble_db: 0.0,
    };

    pub fn new(bass_db: f32, treble_db: f32) -> Result<Self, DspError> {
        if !bass_db.is_finite()
            || !treble_db.is_finite()
            || !(-12.0..=12.0).contains(&bass_db)
            || !(-12.0..=12.0).contains(&treble_db)
        {
            return Err(DspError::InvalidTone);
        }
        Ok(Self { bass_db, treble_db })
    }

    pub fn bass_db(self) -> f32 {
        self.bass_db
    }

    pub fn treble_db(self) -> f32 {
        self.treble_db
    }

    pub fn is_flat(self) -> bool {
        self.bass_db == 0.0 && self.treble_db == 0.0
    }

    /// Conservative headroom for both shelves, in decibels.
    ///
    /// A 6 dB bass boost and 3 dB treble boost reserve 9 dB. This is a
    /// steady-state bound; filter transients or bad source peaks can still
    /// cross full scale, so the final output guard remains required.
    pub fn safe_preamp_db(self) -> f32 {
        -self.bass_db.max(0.0) - self.treble_db.max(0.0)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ToneEq {
    channels: Vec<ToneChannel>,
}

#[derive(Clone, Debug)]
struct ToneChannel {
    bass: Option<DirectForm2Transposed<f64>>,
    treble: Option<DirectForm2Transposed<f64>>,
}

impl ToneEq {
    pub(crate) fn new(format: PcmFormat, controls: ToneControls) -> Result<Self, DspError> {
        let bass = shelf(format.sample_rate(), BASS_HZ, controls.bass_db, false)?;
        let treble = shelf(format.sample_rate(), TREBLE_HZ, controls.treble_db, true)?;
        let channels = (0..format.channels())
            .map(|_| ToneChannel {
                bass: bass.map(DirectForm2Transposed::new),
                treble: treble.map(DirectForm2Transposed::new),
            })
            .collect();
        Ok(Self { channels })
    }

    pub(crate) fn run(&mut self, channel: usize, mut sample: f64) -> f64 {
        let filters = &mut self.channels[channel];
        if let Some(bass) = &mut filters.bass {
            sample = bass.run(sample);
        }
        if let Some(treble) = &mut filters.treble {
            sample = treble.run(sample);
        }
        sample
    }
}

fn shelf(
    sample_rate: u32,
    midpoint_hz: f64,
    gain_db: f32,
    high: bool,
) -> Result<Option<Coefficients<f64>>, DspError> {
    if gain_db == 0.0 {
        return Ok(None);
    }
    let rate = f64::from(sample_rate);
    let midpoint = midpoint_hz.min(rate * MAX_CORNER_FRACTION);
    let kind = if high {
        Type::HighShelf(f64::from(gain_db))
    } else {
        Type::LowShelf(f64::from(gain_db))
    };
    // Q = 1/sqrt(2) corresponds to the monotonic S=1 shelf in the W3C
    // Audio EQ Cookbook. Coefficients are prepared outside the PCM loop.
    let coefficients =
        Coefficients::from_normalized_params(kind, midpoint * 2.0 / rate, Q_BUTTERWORTH_F64)
            .map_err(|_| DspError::InvalidFormat)?;
    Ok(Some(coefficients))
}

#[cfg(test)]
#[path = "tone_tests.rs"]
mod tests;
