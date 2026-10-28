//! Source DSD properties and bounded optional ID3, never derived PCM claims.
use super::{AudioProperties, RawTags, TagError, id3};
use std::fs::File;
/// Read a DSF/DFF source using the same strict extents as the native decoder.
pub fn read(file: &mut File) -> Result<RawTags, TagError> {
    let info = aede_audio::dsd::inspect_source(file)
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
            bitrate_kbps: Some(
                u32::try_from(u64::from(info.sample_rate) * info.channels as u64 / 1000)
                    .map_err(|_| TagError::Malformed("DSD bitrate exceeds catalogue range"))?,
            ),
            lossless: true,
        },
        ..Default::default()
    };
    if let Some(offset) = info.metadata_offset {
        id3::read_id3v2_at(file, offset, info.metadata_bytes, &mut tags)?;
    }
    Ok(tags)
}

/// Derived DSD profile support; source facts remain unchanged in the catalogue.
/// This is not a fresh-file integrity or device-output guarantee.
#[derive(Debug, serde::Serialize)]
pub struct Capabilities {
    /// Known source level or a generic DSD label for other clock families.
    pub label: String,
    /// Native conversion support; absent when source properties are incomplete.
    pub native_pcm: Option<bool>,
    /// Filtered listening PCM clock, distinct from the original one-bit clock.
    pub listening_rate: Option<u32>,
    /// Wideband analysis/spectrogram PCM clock, without listening effects.
    pub measurement_rate: Option<u32>,
    /// Stable unavailability reason; never an invented successful decoder result.
    pub reason: Option<&'static str>,
}
/// Present support from scanned DSD properties without opening or modifying audio.
/// Real opening still validates the retained source before decoding.
pub fn capabilities(properties: &AudioProperties) -> Option<Capabilities> {
    if properties.codec != "dsd" {
        return None;
    }
    let source = properties
        .sample_rate
        .zip(properties.channels)
        .filter(|(rate, channels)| *rate > 0 && *channels > 0 && properties.bit_depth == Some(1));
    let native = source
        .map(|(rate, channels)| aede_audio::dsd::supports_pcm_profile(rate, usize::from(channels)));
    let reason = match (source, native) {
        (_, Some(true)) => None,
        (Some((_, channels)), Some(false)) if !matches!(channels, 1 | 2) => {
            Some("unsupported_channels")
        }
        (Some(_), Some(false)) => Some("unsupported_clock"),
        _ => Some("source_properties_unavailable"),
    };
    Some(Capabilities {
        label: properties.quality_label(),
        native_pcm: native,
        listening_rate: (native == Some(true)).then_some(aede_audio::dsd::LISTENING_SAMPLE_RATE),
        measurement_rate: source
            .filter(|_| native == Some(true))
            .map(|(rate, _)| rate / 8),
        reason,
    })
}

#[cfg(test)]
#[path = "dsd_tests.rs"]
mod tests;
