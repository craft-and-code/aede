//! Progressive complete-track peak envelope, bounded independently of track length.
use super::stream::PcmTrack;
use std::convert::Infallible;
use std::path::Path;
use std::time::{Duration, Instant};
/// Original-sample envelope, before playback normalization, volume or effects.
pub struct Waveform {
    /// Complete source duration derived from decoded frame count.
    pub duration_ms: u64,
    /// Equal frame buckets; the final bucket can be shorter.
    pub peaks: Vec<f32>,
    /// Complete source frames in each bucket except the last.
    pub bucket_frames: u64,
    /// Original source sampling rate.
    pub sample_rate: u32,
}
struct Envelope {
    peaks: Vec<f32>,
    width: u64,
    used: u64,
    peak: f32,
    frames: u64,
}
impl Envelope {
    fn new() -> Self {
        Self {
            peaks: vec![],
            width: 256,
            used: 0,
            peak: 0.,
            frames: 0,
        }
    }
    fn push(&mut self, peak: f32) {
        self.frames += 1;
        self.used += 1;
        self.peak = self.peak.max(peak.abs().min(1.));
        if self.used == self.width {
            self.peaks.push(self.peak);
            self.used = 0;
            self.peak = 0.;
            if self.peaks.len() == 2048 {
                self.peaks = self
                    .peaks
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|v| v[0].max(v[1]))
                    .collect();
                self.width *= 2;
            }
        }
    }
    fn finish(mut self, rate: u32) -> Waveform {
        if self.used > 0 {
            self.peaks.push(self.peak);
        }
        Waveform {
            duration_ms: self.frames.saturating_mul(1000) / u64::from(rate),
            peaks: self.peaks,
            bucket_frames: self.width,
            sample_rate: rate,
        }
    }
}
/// Decode an explicitly selected local source with bounded memory and a wall-clock budget.
/// The caller must validate source identity before and after this read.
pub fn decode(path: &Path, budget: Duration) -> Result<Waveform, String> {
    let mut track = PcmTrack::open(path).map_err(|error| error.to_string())?;
    let format = track.format();
    let start = Instant::now();
    let mut envelope = Envelope::new();
    while let Some(block) = track
        .read_block::<Infallible>(|_| Ok(()))
        .map_err(|error| error.to_string())?
    {
        if start.elapsed() > budget || envelope.frames > u64::from(format.sample_rate()) * 86400 {
            return Err("waveform decode budget exceeded".into());
        }
        for frame in block.samples.chunks_exact(usize::from(format.channels())) {
            envelope.push(frame.iter().copied().map(f32::abs).fold(0., f32::max));
        }
    }
    if envelope.frames == 0 {
        return Err("empty audio source".into());
    }
    Ok(envelope.finish(format.sample_rate()))
}
#[cfg(test)]
#[path = "waveform_tests.rs"]
mod tests;
