//! Source DSD properties and bounded optional ID3, never derived PCM claims.
use super::{AudioProperties, RawTags, TagError, id3};
use std::fs::File;
/// Read a DSF/DFF source using the same strict extents as the native decoder.
pub fn read(file: &mut File) -> Result<RawTags, TagError> {
    let info = aede_audio::dsd::inspect(file)
        .map_err(|_| TagError::Malformed("invalid or unsupported DSF/DFF source"))?;
    let mut tags = RawTags {
        properties: AudioProperties {
            codec: "dsd".into(),
            container: info.container.to_ascii_lowercase(),
            sample_rate: Some(info.sample_rate),
            bit_depth: Some(1),
            channels: Some(info.channels as u16),
            duration_ms: Some(
                (info.samples / u64::from(info.sample_rate)) * 1000
                    + (info.samples % u64::from(info.sample_rate)) * 1000
                        / u64::from(info.sample_rate),
            ),
            bitrate_kbps: Some(info.sample_rate * info.channels as u32 / 1000),
            lossless: true,
        },
        ..Default::default()
    };
    if let Some(offset) = info.metadata_offset {
        id3::read_id3v2_at(file, offset, info.metadata_bytes, &mut tags)?;
    }
    Ok(tags)
}
#[cfg(test)]
#[path = "dsd_tests.rs"]
mod tests;
