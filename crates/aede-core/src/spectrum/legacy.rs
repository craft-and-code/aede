//! Optional codec fallback; FFT, calibration, labels and PNG still use the Rust engine.
use crate::playback::decoder::FfmpegStream;
use flaccompagnon_core::BasicInfo;
use flaccompagnon_core::spectrogram::{AnalysisOptions, Spectrogram};
use std::path::Path;

pub(super) fn analyze(
    path: &Path,
    options: &AnalysisOptions,
    program: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<(Spectrogram, BasicInfo), String> {
    super::measure::analyze(options, cancelled, |sink| {
        visit(path, program, sink, cancelled)
    })
}
fn visit(
    path: &Path,
    program: &str,
    sink: &mut dyn FnMut(&[f32]) -> Result<(), String>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(BasicInfo, u64), String> {
    super::admit(cancelled)?;
    let (mut stream, rate, channels) =
        FfmpegStream::measurement(path, program).map_err(|e| e.to_string())?;
    let mut frames = 0u64;
    loop {
        super::admit(cancelled)?;
        let packet = stream.next_chunk().map_err(|e| e.to_string())?;
        super::admit(cancelled)?;
        let Some(samples) = packet else {
            break;
        };
        if !samples.len().is_multiple_of(usize::from(channels))
            || samples.iter().any(|v| !v.is_finite())
        {
            return Err("external decoder returned incomplete or non-finite PCM".into());
        }
        frames = frames
            .checked_add((samples.len() / usize::from(channels)) as u64)
            .ok_or("spectrogram frame count overflow")?;
        sink(&samples)?;
    }
    if frames == 0 {
        return Err("external decoder returned no audio".into());
    }
    Ok((
        BasicInfo {
            sample_rate: rate,
            channels: usize::from(channels),
            bits: None,
            format: "decoded PCM".into(),
        },
        frames,
    ))
}
