//! Playback loudness measurements over decoded PCM, independent of the output sink.
//!
//! Imported FlacCompagnon values are preferred for a track. Missing values are
//! measured with the Rust ebur128 implementation. Album measurements combine
//! gated block energies for the whole programme, never track LUFS averages.

use std::error::Error;
use std::path::Path;

use ebur128::{EbuR128, Mode};

use crate::analysis::FileAnalysis;
use crate::clock;

use super::decoder::FileDecoder;

/// Integrated loudness and highest inter-sample peak of decoded PCM.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    /// Gated programme loudness in LUFS.
    pub integrated_lufs: f32,
    /// Linear true peak, when measured; may exceed one.
    pub true_peak: Option<f32>,
}

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

fn new_meter(decoder: &FileDecoder) -> Result<Option<EbuR128>, ebur128::Error> {
    // The generic decoder does not expose speaker positions for surround audio.
    if !matches!(decoder.channels(), 1 | 2) {
        return Ok(None);
    }
    EbuR128::new(
        u32::from(decoder.channels()),
        decoder.sample_rate(),
        Mode::I | Mode::TRUE_PEAK,
    )
    .map(Some)
}

fn decode_into(decoder: &mut FileDecoder, meter: &mut EbuR128) -> Result<(), Box<dyn Error>> {
    let channels = usize::from(decoder.channels());
    let mut samples = vec![0.0; 4096 * channels];
    loop {
        let frames = decoder.read_frames(&mut samples)?;
        if frames == 0 {
            break;
        }
        meter.add_frames_f32(&samples[..frames * channels])?;
    }
    Ok(())
}

fn peak_of(meter: &EbuR128) -> Result<f32, ebur128::Error> {
    let mut peak = 0.0f32;
    for channel in 0..meter.channels() {
        peak = peak.max(meter.true_peak(channel)? as f32);
    }
    Ok(peak)
}

fn result(loudness: f64, peak: f32) -> Option<Measurement> {
    (loudness.is_finite() && (-100.0..=20.0).contains(&loudness) && peak.is_finite()).then_some(
        Measurement {
            integrated_lufs: loudness as f32,
            true_peak: Some(peak),
        },
    )
}

/// Measure a track once before output, so playback never has to wait mid-track.
pub fn measure_track(path: &Path) -> Result<Option<Measurement>, Box<dyn Error>> {
    let mut decoder = FileDecoder::open(path)?;
    let Some(mut meter) = new_meter(&decoder)? else {
        return Ok(None);
    };
    decode_into(&mut decoder, &mut meter)?;
    Ok(result(meter.loudness_global()?, peak_of(&meter)?))
}

/// Measure an ordered album programme. One meter spans matching formats;
/// different formats retain independent meters, then share one gated result.
pub fn measure_programme(paths: &[&Path]) -> Result<Option<Measurement>, Box<dyn Error>> {
    if paths.is_empty() {
        return Ok(None);
    }
    let mut meters: Vec<EbuR128> = Vec::new();
    for path in paths {
        let mut decoder = FileDecoder::open(path)?;
        if !matches!(decoder.channels(), 1 | 2) {
            return Ok(None);
        }
        let reuse = meters.last().is_some_and(|meter| {
            meter.rate() == decoder.sample_rate()
                && meter.channels() == u32::from(decoder.channels())
        });
        if !reuse {
            meters.push(new_meter(&decoder)?.ok_or("unsupported channel layout")?);
        }
        decode_into(&mut decoder, meters.last_mut().ok_or("no programme meter")?)?;
    }
    let loudness = EbuR128::loudness_global_multiple(meters.iter())?;
    let peak = meters.iter().try_fold(0.0f32, |peak, meter| {
        Ok::<_, ebur128::Error>(peak.max(peak_of(meter)?))
    })?;
    Ok(result(loudness, peak))
}

#[cfg(test)]
#[path = "loudness_tests.rs"]
mod tests;
