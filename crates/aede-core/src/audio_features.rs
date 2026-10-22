//! Versioned original-audio features beside the catalogue, never inside source files or personal opinions.
use crate::playback::stream::PcmTrack;
use aede_moods::{Extractor, Features, RATE};
use std::path::Path;
use std::time::{Duration, Instant};
mod cache;
pub use cache::{InputIdentity, Record, load, save};
/// Decode/resample with existing Aède and FlacCompagnon primitives into bounded descriptor state.
/// Checkpoints may wait for foreground playback, or refuse cancellation/source replacement.
/// The active computation budget excludes checkpoint waiting; blocked OS reads retain platform limits.
pub fn extract(
    path: &Path,
    mut checkpoint: impl FnMut() -> Result<(), String>,
) -> Result<Features, String> {
    checkpoint()?;
    let mut track = PcmTrack::open_native(path).map_err(|e| e.to_string())?;
    let format = track.source_format();
    if !(1..=2).contains(&format.channels()) || !(8000..=192000).contains(&format.sample_rate()) {
        return Err("source outside native mono/stereo acoustic profile".into());
    }
    track.set_output_rate(RATE).map_err(|e| e.to_string())?;
    let origin = Instant::now();
    extract_source(
        &mut track,
        usize::from(format.channels()),
        checkpoint,
        || origin.elapsed(),
    )
}

trait Source {
    fn read_samples(&mut self) -> Result<Option<&[f32]>, String>;
}
impl Source for PcmTrack {
    fn read_samples(&mut self) -> Result<Option<&[f32]>, String> {
        self.read_block::<std::convert::Infallible>(|_| Ok(()))
            .map(|block| block.map(|block| block.samples))
            .map_err(|error| error.to_string())
    }
}

fn check_budget(active: Duration) -> Result<(), String> {
    if active > Duration::from_secs(180) {
        Err("acoustic active computation budget exceeded".into())
    } else {
        Ok(())
    }
}

// Keep budget accounting on the same path for native decoding and controlled clocks.
fn extract_source(
    track: &mut impl Source,
    channels: usize,
    mut checkpoint: impl FnMut() -> Result<(), String>,
    mut now: impl FnMut() -> Duration,
) -> Result<Features, String> {
    let mut extractor = Extractor::new(channels)?;
    let mut active = Duration::ZERO;
    loop {
        checkpoint()?;
        let step = now();
        let samples = track.read_samples()?;
        active += now().saturating_sub(step);
        check_budget(active)?;
        let Some(samples) = samples else {
            break;
        };
        let mut parked = Duration::ZERO;
        let step = now();
        extractor.push(samples, || {
            let before = now();
            let result = checkpoint();
            parked += now().saturating_sub(before);
            result
        })?;
        active += now().saturating_sub(step).saturating_sub(parked);
        check_budget(active)?;
    }
    checkpoint()?;
    extractor.finish()
}
#[cfg(test)]
#[path = "audio_features_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "audio_features_budget_tests.rs"]
mod budget_tests;
