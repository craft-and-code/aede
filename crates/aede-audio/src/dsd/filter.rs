//! Windowed-sinc FIR decimation, derived directly from the ideal low-pass.
//! No borrowed codec implementation or generated coefficient dependency.
use std::f64::consts::PI;
use std::sync::OnceLock;

const TAPS: usize = 256;
const BYTES: usize = TAPS / 8;

fn coefficients(taps: usize, cutoff: f64) -> Vec<f64> {
    let mut values: Vec<_> = (0..taps)
        .map(|i| {
            let x = i as f64 - (taps - 1) as f64 / 2.0;
            let ideal = (2.0 * PI * cutoff * x).sin() / (PI * x);
            // Blackman window: finite support with strong out-of-band rejection.
            let angle = 2.0 * PI * i as f64 / (taps - 1) as f64;
            ideal * (0.42 - 0.5 * angle.cos() + 0.08 * (2.0 * angle).cos())
        })
        .collect();
    let sum: f64 = values.iter().sum();
    values.iter_mut().for_each(|v| *v /= sum);
    values
}
fn table() -> &'static Vec<[f64; 256]> {
    static TABLE: OnceLock<Vec<[f64; 256]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        // DSD64: nominal 127 kHz cutoff, preserving the 20–75 kHz
        // evidence used by analysis. DSD128 scales with its source clock.
        let taps = coefficients(TAPS, 0.045);
        (0..BYTES)
            .map(|group| {
                std::array::from_fn(|byte| {
                    (0..8)
                        .map(|bit| {
                            taps[group * 8 + bit] * if byte & (1 << bit) == 0 { -1.0 } else { 1.0 }
                        })
                        .sum()
                })
            })
            .collect()
    })
}

pub(super) struct BitFilter {
    history: [u8; BYTES],
    next: usize,
}
impl BitFilter {
    pub fn new() -> Self {
        let _ = table();
        Self {
            history: [0x69; BYTES],
            next: 0,
        }
    }
    pub fn push(&mut self, byte: u8) -> f32 {
        self.history[self.next] = byte;
        self.next = (self.next + 1) % BYTES;
        // Index 0 of the convolution is the newest bit, bit 7 of the
        // newest byte. Reversing each chronological byte makes it explicit.
        let mut result = 0.0;
        for (group, values) in table().iter().enumerate() {
            let index = (self.next + BYTES - 1 - group) % BYTES;
            result += values[self.history[index].reverse_bits() as usize];
        }
        result as f32
    }
}

pub(super) struct ListeningFilter {
    taps: Vec<f64>,
    history: Vec<Vec<f32>>,
    next: usize,
    fed: usize,
    factor: usize,
}
impl ListeningFilter {
    pub fn new(rate: u32, channels: usize) -> Self {
        // Both listening profiles yield 88.2 kHz. The extra low-pass
        // rejects ultrasonic shaped noise instead of feeding it to a DAC.
        let factor = (rate / super::LISTENING_SAMPLE_RATE) as usize;
        let taps = coefficients(64 * factor, 30_000.0 / f64::from(rate));
        Self {
            history: vec![vec![0.0; taps.len()]; channels],
            taps,
            next: 0,
            fed: 0,
            factor,
        }
    }
    pub fn push(&mut self, frame: &[f32], out: &mut [f32]) -> bool {
        for (channel, &sample) in self.history.iter_mut().zip(frame) {
            channel[self.next] = sample;
        }
        self.next = (self.next + 1) % self.taps.len();
        self.fed += 1;
        if self.fed <= self.taps.len() / 2
            || !(self.fed - self.taps.len() / 2 - 1).is_multiple_of(self.factor)
        {
            return false;
        }
        // Keep only a bounded phase counter, even on very long files.
        self.fed = self.taps.len() / 2 + 1;
        for (channel, sample) in self.history.iter().zip(out) {
            *sample = self
                .taps
                .iter()
                .enumerate()
                .map(|(i, tap)| {
                    f64::from(channel[(self.next + self.taps.len() - 1 - i) % self.taps.len()])
                        * tap
                })
                .sum::<f64>() as f32;
        }
        true
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
