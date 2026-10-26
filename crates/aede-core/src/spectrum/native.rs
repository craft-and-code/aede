//! Aède's existing verified FLAC reader covers 32-bit modes unsupported by claxon.
use crate::playback::decoder::FileDecoder;
use flaccompagnon_core::{
    BasicInfo,
    spectrogram::{AnalysisOptions, Spectrogram},
};
use std::path::Path;
pub(super) fn analyze(
    path: &Path,
    options: &AnalysisOptions,
    cancelled: &dyn Fn() -> bool,
) -> Result<(Spectrogram, BasicInfo), String> {
    super::measure::analyze(options, cancelled, |sink| {
        super::admit(cancelled)?;
        let mut decoder = FileDecoder::open_native(path).map_err(|e| e.to_string())?;
        let channels = usize::from(decoder.channels());
        let info = BasicInfo {
            sample_rate: decoder.sample_rate(),
            channels,
            bits: Some(32),
            format: "FLAC".into(),
        };
        let mut packet = vec![0.0; 4096 * channels];
        let mut frames = 0u64;
        loop {
            super::admit(cancelled)?;
            let read = decoder
                .read_frames(&mut packet)
                .map_err(|e| e.to_string())?;
            super::admit(cancelled)?;
            if read == 0 {
                break;
            }
            sink(&packet[..read * channels])?;
            frames = frames
                .checked_add(read as u64)
                .ok_or("spectrogram frame count overflow")?;
        }
        if frames == 0 {
            return Err("no audio decoded".into());
        }
        Ok((info, frames))
    })
}
