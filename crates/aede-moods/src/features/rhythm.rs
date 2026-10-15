use super::{HOP, RATE};
// Fixed 24-second onset windows; histograms retain bounded whole-track pulse evidence.
#[derive(Default)]
pub(super) struct Rhythm {
    envelope: Vec<f32>,
    estimates: Vec<(f32, f32)>,
}
impl Rhythm {
    pub(super) fn push(&mut self, value: f32) {
        self.envelope.push(value);
        if self.envelope.len() == 1024 {
            self.measure();
            self.envelope.drain(..512);
        }
    }
    fn measure(&mut self) {
        if self.envelope.len() < 256 {
            return;
        }
        let mean = self.envelope.iter().sum::<f32>() / self.envelope.len() as f32;
        if mean < 0.001 {
            return;
        }
        let energy = self
            .envelope
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f32>();
        if energy < 1e-6 {
            return;
        }
        let rate = RATE as f32 / HOP as f32;
        let low = (rate * 60. / 220.).ceil() as usize;
        let high = (rate * 60. / 50.).floor() as usize;
        let mut scores = vec![0.; high + 2];
        for (lag, score) in scores
            .iter_mut()
            .enumerate()
            .take(high + 2)
            .skip(low.saturating_sub(1))
        {
            let a = &self.envelope[lag..];
            let b = &self.envelope[..self.envelope.len() - lag];
            let numerator = a
                .iter()
                .zip(b)
                .map(|(a, b)| (a - mean) * (b - mean))
                .sum::<f32>();
            let denom = (a.iter().map(|v| (v - mean).powi(2)).sum::<f32>()
                * b.iter().map(|v| (v - mean).powi(2)).sum::<f32>())
            .sqrt();
            *score = if denom > 1e-9 {
                (numerator / denom).clamp(0., 1.)
            } else {
                0.
            };
        }
        let peak = scores[low..=high].iter().copied().fold(0., f32::max);
        if peak < 0.5 {
            return;
        }
        if let Some(lag) = (low..=high).find(|&n| {
            scores[n] >= peak * 0.8 && scores[n] >= scores[n - 1] && scores[n] >= scores[n + 1]
        }) {
            let denominator = scores[lag - 1] - 2. * scores[lag] + scores[lag + 1];
            let correction = if denominator.abs() > 1e-6 {
                0.5 * (scores[lag - 1] - scores[lag + 1]) / denominator
            } else {
                0.
            };
            let bpm = rate * 60. / (lag as f32 + correction.clamp(-0.5, 0.5));
            if (50.0..=220.).contains(&bpm) {
                self.estimates.push((bpm, scores[lag]));
            }
        }
        // Keep a bounded representative reservoir, rather than an unbounded duration journal.
        if self.estimates.len() > 128 {
            self.estimates = self
                .estimates
                .chunks(2)
                .map(|pair| {
                    pair.get(1).map_or(pair[0], |second| {
                        ((pair[0].0 + second.0) / 2., (pair[0].1 + second.1) / 2.)
                    })
                })
                .collect();
        }
    }
    pub(super) fn finish(mut self) -> (Option<f32>, Option<f32>) {
        self.measure();
        if self.estimates.is_empty() {
            return (None, None);
        }
        self.estimates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let selected = self.estimates[self.estimates.len() / 2];
        (Some(selected.0), Some(selected.1))
    }
}
