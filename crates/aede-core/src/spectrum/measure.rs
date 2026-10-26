//! One bounded two-pass measurement policy for host-selected PCM readers.
use flaccompagnon_core::{
    BasicInfo,
    spectrogram::{AnalysisOptions, SignalInfo, Spectrogram, SpectrogramBuilder},
};
type Sink<'a> = dyn FnMut(&[f32]) -> Result<(), String> + 'a;
pub(super) fn analyze(
    options: &AnalysisOptions,
    cancelled: &dyn Fn() -> bool,
    mut visit: impl FnMut(&mut Sink<'_>) -> Result<(BasicInfo, u64), String>,
) -> Result<(Spectrogram, BasicInfo), String> {
    let (info, frames) = visit(&mut |_| Ok(()))?;
    let mut builder = SpectrogramBuilder::new(
        SignalInfo {
            sample_rate: info.sample_rate,
            channels: info.channels,
            frames,
        },
        options.clone(),
    )
    .map_err(|e| e.to_string())?;
    let (again, count) = visit(&mut |samples| {
        builder
            .push_interleaved_cancellable(samples, cancelled)
            .map_err(|e| e.to_string())
    })?;
    if info.sample_rate != again.sample_rate || info.channels != again.channels || frames != count {
        return Err("decoder format changed between spectrogram passes".into());
    }
    Ok((
        builder
            .finish_cancellable(cancelled)
            .map_err(|e| e.to_string())?,
        info,
    ))
}
