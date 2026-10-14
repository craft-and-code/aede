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
    let mut extractor = Extractor::new(usize::from(format.channels()))?;
    let mut active = Duration::ZERO;
    loop {
        checkpoint()?;
        let step = Instant::now();
        let block = track
            .read_block::<std::convert::Infallible>(|_| Ok(()))
            .map_err(|e| e.to_string())?;
        active += step.elapsed();
        let Some(block) = block else {
            break;
        };
        let mut parked = Duration::ZERO;
        let step = Instant::now();
        extractor.push(block.samples, || {
            let before = Instant::now();
            let result = checkpoint();
            parked += before.elapsed();
            result
        })?;
        active += step.elapsed().saturating_sub(parked);
        if active > Duration::from_secs(180) {
            return Err("acoustic active computation budget exceeded".into());
        }
    }
    checkpoint()?;
    extractor.finish()
}
#[cfg(test)]
#[path = "audio_features_tests.rs"]
mod tests;
