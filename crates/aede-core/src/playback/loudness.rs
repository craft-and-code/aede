//! Playback loudness measurements over decoded PCM, independent of the output sink.
//!
//! Imported FlacCompagnon values are preferred for a track. Missing values are
//! measured over decoded PCM by `aede-dsp`. Album measurements combine gated
//! block energies for the whole programme, never track LUFS averages.

use std::error::Error;
use std::path::Path;

use aede_dsp::{PcmFormat, loudness::LoudnessProgramme};

use crate::analysis::FileAnalysis;
use crate::clock;

use super::decoder::FileDecoder;

pub use aede_dsp::loudness::Measurement;

/// One version of a file's measured playback loudness.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedTrack {
    /// File size at measurement time.
    pub size: u64,
    /// File modification time at measurement time.
    pub mtime: u64,
    /// `None` records silence or an unsupported channel layout.
    pub measurement: Option<Measurement>,
}

/// A file's identity within an ordered album programme.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProgrammeFile {
    /// Canonical audio file path.
    pub path: String,
    /// File size at measurement time.
    pub size: u64,
    /// File modification time at measurement time.
    pub mtime: u64,
}

/// One programme loudness measurement, invalidated when any member changes.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedProgramme {
    /// Ordered file identities in the programme.
    pub files: Vec<ProgrammeFile>,
    /// `None` records silence or an unsupported layout.
    pub measurement: Option<Measurement>,
}

/// Read the identity used by both the imported-analysis and playback caches.
pub fn identity(path: &Path) -> Result<ProgrammeFile, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    Ok(ProgrammeFile {
        path: path.to_string_lossy().into_owned(),
        size: metadata.len(),
        mtime: clock::mtime_seconds(&metadata),
    })
}

/// Reuse an attributed analysis only while it describes these exact bytes.
/// A missing true peak stays unknown so the output stage can choose its policy.
pub fn from_flaccompagnon(analyses: &[FileAnalysis], file: &ProgrammeFile) -> Option<Measurement> {
    analyses.iter().rev().find_map(|analysis| {
        if analysis.path != file.path
            || analysis.source != "flaccompagnon"
            || analysis.error.is_some()
            || !analysis.still_applies(file.size, file.mtime)
        {
            return None;
        }
        let loudness = analysis.integrated_lufs?;
        if !loudness.is_finite() || !(-100.0..=20.0).contains(&loudness) {
            return None;
        }
        let peak = analysis
            .true_peak_dbtp
            .filter(|value| value.is_finite())
            .map(|db| 10f64.powf(db / 20.0) as f32)
            .filter(|peak| peak.is_finite() && *peak >= 0.0);
        Some(Measurement {
            integrated_lufs: loudness as f32,
            true_peak: peak,
        })
    })
}

fn decode_into(
    decoder: &mut FileDecoder,
    meter: &mut LoudnessProgramme,
) -> Result<(), Box<dyn Error>> {
    let channels = usize::from(decoder.channels());
    let format = PcmFormat::new(decoder.sample_rate(), decoder.channels())?;
    let mut samples = vec![0.0; 4096 * channels];
    loop {
        let frames = decoder.read_frames(&mut samples)?;
        if frames == 0 {
            break;
        }
        meter.push(format, &samples[..frames * channels])?;
    }
    Ok(())
}

/// Measure a track once before output, so playback never has to wait mid-track.
pub fn measure_track(path: &Path) -> Result<Option<Measurement>, Box<dyn Error>> {
    let mut decoder = FileDecoder::open(path)?;
    if !matches!(decoder.channels(), 1 | 2) {
        return Ok(None);
    }
    let mut meter = LoudnessProgramme::new();
    decode_into(&mut decoder, &mut meter)?;
    Ok(meter.measurement()?)
}

/// Measure an ordered album programme. One meter spans matching formats;
/// different formats retain independent meters, then share one gated result.
pub fn measure_programme(paths: &[&Path]) -> Result<Option<Measurement>, Box<dyn Error>> {
    if paths.is_empty() {
        return Ok(None);
    }
    let mut meter = LoudnessProgramme::new();
    for path in paths {
        let mut decoder = FileDecoder::open(path)?;
        if !matches!(decoder.channels(), 1 | 2) {
            return Ok(None);
        }
        decode_into(&mut decoder, &mut meter)?;
    }
    Ok(meter.measurement()?)
}

#[cfg(test)]
#[path = "loudness_tests.rs"]
mod tests;
