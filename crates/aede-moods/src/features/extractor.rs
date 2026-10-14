use super::{Features, HOP, Moments, PROFILE, RATE, WINDOW, rhythm::Rhythm};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::sync::Arc;
/// Resident FFT/scratch/window buffers; no allocation grows with source duration.
pub struct Extractor {
    channels: usize,
    frames: u64,
    windows: u64,
    position: usize,
    since_window: usize,
    ring: [Vec<f32>; 2],
    previous: [f32; 2],
    crossings: u64,
    squares: f64,
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Complex<f32>>,
    work: Vec<Complex<f32>>,
    hann: Vec<f32>,
    power: Vec<f64>,
    previous_power: Vec<f64>,
    centroid: Moments,
    rolloff: Moments,
    flatness: Moments,
    chroma: [f64; 12],
    chroma_bins: Vec<Option<usize>>,
    rhythm: Rhythm,
}
impl Extractor {
    /// Accept mono/stereo at RATE; a decoder/resampler belongs to the caller.
    pub fn new(channels: usize) -> Result<Self, String> {
        if !(1..=2).contains(&channels) {
            return Err("acoustic profile requires mono or stereo".into());
        }
        let fft = FftPlanner::<f32>::new().plan_fft_forward(WINDOW);
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
        Ok(Self {
            channels,
            frames: 0,
            windows: 0,
            position: 0,
            since_window: 0,
            ring: [vec![0.; WINDOW], vec![0.; WINDOW]],
            previous: [0.; 2],
            crossings: 0,
            squares: 0.,
            fft,
            scratch,
            work: vec![Complex::default(); WINDOW],
            hann: (0..WINDOW)
                .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / (WINDOW - 1) as f32).cos())
                .collect(),
            power: vec![0.; WINDOW / 2 + 1],
            previous_power: vec![0.; WINDOW / 2 + 1],
            centroid: Moments::default(),
            rolloff: Moments::default(),
            flatness: Moments::default(),
            chroma: [0.; 12],
            chroma_bins: (0..=WINDOW / 2)
                .map(|bin| {
                    let frequency = bin as f64 * f64::from(RATE) / WINDOW as f64;
                    (40.0..=5000.).contains(&frequency).then(|| {
                        let note = (69. + 12. * (frequency / 440.).log2()).round() as i32;
                        note.rem_euclid(12) as usize
                    })
                })
                .collect(),
            rhythm: Rhythm::default(),
        })
    }
    /// Check cancellation/foreground activity before ingesting a packet and each transform.
    pub fn push(
        &mut self,
        samples: &[f32],
        mut checkpoint: impl FnMut() -> Result<(), String>,
    ) -> Result<(), String> {
        checkpoint()?;
        if !samples.len().is_multiple_of(self.channels)
            || samples.iter().any(|x| !x.is_finite() || x.abs() > 16.)
        {
            return Err("invalid acoustic PCM packet".into());
        }
        for frame in samples.chunks_exact(self.channels) {
            self.frames += 1;
            if self.frames > u64::from(RATE) * 86400 {
                return Err("acoustic duration budget exceeded".into());
            }
            for (channel, &sample) in frame.iter().enumerate() {
                if self.frames > 1
                    && ((self.previous[channel] < 0. && sample >= 0.)
                        || (self.previous[channel] >= 0. && sample < 0.))
                {
                    self.crossings += 1;
                }
                self.previous[channel] = sample;
                self.squares += f64::from(sample).powi(2);
                self.ring[channel][self.position] = sample;
            }
            self.position = (self.position + 1) % WINDOW;
            self.since_window += 1;
            if self.frames >= WINDOW as u64 && self.since_window >= HOP {
                checkpoint()?;
                self.window();
                self.since_window = 0;
            }
        }
        Ok(())
    }
    fn window(&mut self) {
        self.windows += 1;
        self.power.fill(0.);
        for channel in 0..self.channels {
            for n in 0..WINDOW {
                self.work[n] = Complex::new(
                    self.ring[channel][(self.position + n) % WINDOW] * self.hann[n],
                    0.,
                );
            }
            self.fft
                .process_with_scratch(&mut self.work, &mut self.scratch);
            for (power, bin) in self.power.iter_mut().zip(&self.work) {
                *power += f64::from(bin.norm_sqr()) / self.channels as f64;
            }
        }
        let sum = self.power[1..].iter().sum::<f64>();
        if sum < 1e-10 {
            self.rhythm.push(0.);
            self.previous_power.copy_from_slice(&self.power);
            return;
        }
        let bin_hz = f64::from(RATE) / WINDOW as f64;
        let centroid = self
            .power
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, p)| i as f64 * bin_hz * p)
            .sum::<f64>()
            / sum;
        let mut acc = 0.;
        let mut rolloff = 0.;
        for (bin, &value) in self.power.iter().enumerate().skip(1) {
            acc += value;
            if acc >= sum * 0.85 {
                rolloff = bin as f64 * bin_hz;
                break;
            }
        }
        let floor = sum * 1e-12;
        let geometric = (self.power[1..]
            .iter()
            .map(|p| p.max(floor).ln())
            .sum::<f64>()
            / (self.power.len() - 1) as f64)
            .exp();
        self.centroid.push(centroid);
        self.rolloff.push(rolloff);
        self.flatness
            .push((geometric / (sum / (self.power.len() - 1) as f64)).clamp(0., 1.));
        let flux = self
            .power
            .iter()
            .zip(&self.previous_power)
            .map(|(a, b)| (a - b).max(0.))
            .sum::<f64>()
            / sum;
        self.rhythm
            .push(if flux > 0.01 { flux.min(1.) as f32 } else { 0. });
        self.previous_power.copy_from_slice(&self.power);
        for (note, value) in self.chroma_bins.iter().zip(&self.power) {
            if let Some(note) = note {
                self.chroma[*note] += value / sum;
            }
        }
    }
    /// Complete one source, retaining unknown tempo and RMS for silence.
    pub fn finish(mut self) -> Result<Features, String> {
        if self.frames == 0 {
            return Err("empty acoustic source".into());
        }
        if self.windows == 0 {
            self.window();
        }
        let chroma_sum = self.chroma.iter().sum::<f64>();
        let chroma = self.chroma.map(|value| {
            if chroma_sum > 0. {
                (value / chroma_sum) as f32
            } else {
                0.
            }
        });
        let (tempo_bpm, periodicity) = self.rhythm.finish();
        let features = Features {
            profile: PROFILE,
            frames: self.frames,
            windows: self.windows,
            centroid_hz: self.centroid.finish(),
            rolloff_hz: self.rolloff.finish(),
            flatness: self.flatness.finish(),
            zero_crossing: self.crossings as f32 / (self.frames as f32 * self.channels as f32),
            chroma,
            tempo_bpm,
            periodicity,
            rms_dbfs: if self.squares > 1e-20 {
                Some(
                    (10. * (self.squares / (self.frames as f64 * self.channels as f64)).log10())
                        .max(-240.) as f32,
                )
            } else {
                None
            },
        };
        if !features.valid() {
            return Err("invalid acoustic descriptor".into());
        }
        Ok(features)
    }
}
