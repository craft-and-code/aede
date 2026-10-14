//! Request-local acoustic neighbors around explicit personal examples; no emotional labels are inferred.
use crate::Features;
const DIMENSIONS: usize = 7;
fn vector(features: &Features) -> [Option<f32>; DIMENSIONS] {
    let nyquist = crate::RATE as f32 / 2.;
    [
        Some(features.centroid_hz.mean / nyquist),
        Some(features.centroid_hz.deviation / nyquist),
        Some(features.rolloff_hz.mean / nyquist),
        Some(features.flatness.mean),
        Some(features.zero_crossing),
        features.tempo_bpm.map(|bpm| (bpm / 120.).log2() / 3.),
        features.periodicity,
    ]
}
/// Bounded personal exemplars, calibrated with capped inverse variance per descriptor dimension.
/// Values are normalized signal distances, not probabilities, moods or genre predictions.
pub struct AcousticModel {
    seeds: Vec<[Option<f32>; DIMENSIONS]>,
    weights: [f32; DIMENSIONS],
    radius: f32,
}
impl AcousticModel {
    /// Require two to sixteen complete usable same-profile examples. Never silently fill missing tempo.
    pub fn new(examples: &[&Features]) -> Option<Self> {
        if !(2..=16).contains(&examples.len()) || examples.iter().any(|row| !row.usable()) {
            return None;
        }
        let seeds = examples.iter().map(|row| vector(row)).collect::<Vec<_>>();
        let base = [1., 0.6, 0.8, 0.7, 0.3, 0.8, 0.2];
        let mut weights = [0.; DIMENSIONS];
        for i in 0..DIMENSIONS {
            let values = seeds.iter().filter_map(|row| row[i]).collect::<Vec<_>>();
            if values.len() < 2 {
                continue;
            }
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            let variance =
                values.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / values.len() as f32;
            weights[i] = base[i] / (0.01 + variance);
        }
        let mut model = Self {
            seeds,
            weights,
            radius: 0.,
        };
        let mut spread = 0_f32;
        for i in 0..model.seeds.len() {
            let nearest = model
                .seeds
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .filter_map(|(_, other)| model.between(&model.seeds[i], other))
                .fold(f32::INFINITY, f32::min);
            spread = spread.max(nearest);
        }
        model.radius = (spread * 1.25 + 0.025).clamp(0.045, 0.18);
        Some(model)
    }
    fn between(&self, a: &[Option<f32>; DIMENSIONS], b: &[Option<f32>; DIMENSIONS]) -> Option<f32> {
        let mut sum = 0.;
        let mut weight = 0.;
        let mut known = 0;
        for i in 0..DIMENSIONS {
            if let (Some(a), Some(b)) = (a[i], b[i])
                && self.weights[i] > 0.
            {
                sum += self.weights[i] * (a - b).powi(2);
                weight += self.weights[i];
                known += 1;
            }
        }
        (known >= 4 && weight > 0.).then(|| (sum / weight).sqrt())
    }
    /// Nearest usable example distance; silent, invalid or insufficient shared evidence stays unknown.
    pub fn distance(&self, features: &Features) -> Option<f32> {
        if !features.usable() {
            return None;
        }
        let current = vector(features);
        self.seeds
            .iter()
            .filter_map(|seed| self.between(seed, &current))
            .min_by(f32::total_cmp)
    }
    /// Capped acceptance radius calibrated from the personal examples, independently of mastering gain.
    pub fn radius(&self) -> f32 {
        self.radius
    }
    /// Number of usable personal examples admitted by this model.
    pub fn examples(&self) -> usize {
        self.seeds.len()
    }
}
