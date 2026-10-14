//! Bounded streaming acoustic descriptors, independent of decoders and personal labels.
use serde::{Deserialize, Serialize};
mod extractor;
mod rhythm;
pub use extractor::Extractor;
/// Increment when any measurement, normalization or source profile changes.
pub const PROFILE: u32 = 1;
/// Fixed analysis rate; the caller uses Aède's bandlimited streaming resampler.
pub const RATE: u32 = 22050;
pub(super) const WINDOW: usize = 2048;
pub(super) const HOP: usize = 512;
/// Finite frame-distribution summary; absence is represented by the enclosing optional value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Statistics {
    /// Arithmetic mean of non-silent analysis windows.
    pub mean: f32,
    /// Population standard deviation of the same windows.
    pub deviation: f32,
}
/// Original audio measurements, not an emotion, genre, key or quality verdict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Features {
    /// Measurement profile, independent of the file/cache schema.
    pub profile: u32,
    /// Frames traversed at the fixed analysis sampling rate.
    pub frames: u64,
    /// Number of complete analysis windows.
    pub windows: u64,
    /// Energy-weighted spectral centre in hertz.
    pub centroid_hz: Statistics,
    /// Frequency below which 85 percent of spectral power lies.
    pub rolloff_hz: Statistics,
    /// Geometric/arithmetic power ratio, zero for strongly tonal frames.
    pub flatness: Statistics,
    /// Original per-channel zero-crossing fraction, averaged without phase cancellation.
    pub zero_crossing: f32,
    /// Normalized twelve pitch-class energy bins; does not estimate a musical key.
    pub chroma: [f32; 12],
    /// Estimated pulse rate; half/double tempo ambiguity remains possible.
    pub tempo_bpm: Option<f32>,
    /// Periodicity of the accepted onset envelope, not emotional confidence.
    pub periodicity: Option<f32>,
    /// Sample RMS in dBFS; never interpreted as emotional energy or LUFS.
    pub rms_dbfs: Option<f32>,
}
impl Features {
    /// Refuse unknown profiles, malformed ranges and inconsistent optional measurements.
    pub fn valid(&self) -> bool {
        let spectral = |s: &Statistics, max: f32| {
            s.mean.is_finite()
                && s.deviation.is_finite()
                && s.mean >= 0.
                && s.mean <= max
                && s.deviation >= 0.
                && s.deviation <= max
        };
        self.profile == PROFILE
            && self.frames > 0
            && self.frames <= u64::from(RATE) * 86400
            && self.windows > 0
            && self.windows <= self.frames / HOP as u64 + 1
            && spectral(&self.centroid_hz, RATE as f32 / 2.)
            && spectral(&self.rolloff_hz, RATE as f32 / 2.)
            && spectral(&self.flatness, 1.)
            && self.zero_crossing.is_finite()
            && (0.0..=1.).contains(&self.zero_crossing)
            && self
                .chroma
                .iter()
                .all(|x| x.is_finite() && (0.0..=1.).contains(x))
            && self.chroma.iter().sum::<f32>() <= 1.001
            && self
                .rms_dbfs
                .is_none_or(|x| x.is_finite() && (-240.0..=24.1).contains(&x))
            && self
                .tempo_bpm
                .is_none_or(|x| x.is_finite() && (50.0..=220.).contains(&x))
            && self
                .periodicity
                .is_none_or(|x| x.is_finite() && (0.0..=1.).contains(&x))
            && self.tempo_bpm.is_some() == self.periodicity.is_some()
    }
    /// Silent/too-short sources cannot serve as acoustic exemplars.
    pub fn usable(&self) -> bool {
        self.valid() && self.windows >= 8 && self.rms_dbfs.is_some_and(|x| x > -80.)
    }
}
#[derive(Default)]
pub(super) struct Moments {
    count: u64,
    mean: f64,
    m2: f64,
}
impl Moments {
    pub(super) fn push(&mut self, value: f64) {
        self.count += 1;
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (value - self.mean);
    }
    pub(super) fn finish(&self) -> Statistics {
        Statistics {
            mean: self.mean as f32,
            deviation: if self.count > 0 {
                (self.m2 / self.count as f64).max(0.).sqrt() as f32
            } else {
                0.
            },
        }
    }
}
