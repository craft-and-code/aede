//! Exact channel association and neutral-control admission, independent of the source codec.
use crate::dsp::ChannelLayout;
/// Recognize only unambiguous mono or front-left/front-right layouts.
pub fn channel_layout(text: &str) -> Option<ChannelLayout> {
    match text.trim() {
        "MONO" | "FC" => Some(ChannelLayout::MONO),
        "FL FR" => Some(ChannelLayout::STEREO),
        _ => None,
    }
}

/// Read-only observations of the selected hardware channel-map control.
pub struct ControlMap<'a> {
    pub card: i32,
    pub device: u32,
    pub subdevice: u32,
    pub index: u32,
    pub integer: bool,
    pub positions: &'a [i32],
}

/// Require the physical control identity and positions to match configured I/O.
pub fn verify_control_map(
    selected: (i32, u32, u32),
    pcm: ChannelLayout,
    actual: Option<ControlMap<'_>>,
) -> Result<(), String> {
    let actual = actual.ok_or("selected ALSA hardware channel-map control is missing")?;
    if selected.0 < 0
        || actual.card != selected.0
        || actual.device != selected.1
        || actual.index != selected.2
        || actual.subdevice != 0
        || !actual.integer
    {
        return Err("ALSA hardware channel-map control identity or type is unverified".to_owned());
    }
    // ALSA's stable channel-position IDs: MONO=2, FL=3, FR=4, FC=7.
    // Extra channel flags/positions are not assumptions about this route.
    let hardware = match actual.positions {
        [2] | [7] => ChannelLayout::MONO,
        [3, 4] => ChannelLayout::STEREO,
        _ => {
            return Err(
                "ALSA hardware channel-map positions are unknown or unsupported".to_owned(),
            );
        }
    };
    if hardware != pcm {
        return Err("configured ALSA PCM mapping differs from the hardware control".to_owned());
    }
    Ok(())
}

// Admit only standard controls with a directly observable neutral meaning.
// Unknown effects, enumerations, routing and vendor controls are refusals.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ControlKind {
    Integer,
    Boolean,
    Unknown,
}

/// Accept only standard switches and exact zero-dB volume observations.
pub fn neutral_control(name: &str, kind: ControlKind, values: &[i64], db: Option<&[i64]>) -> bool {
    match name {
        "PCM Playback Volume"
        | "Master Playback Volume"
        | "Speaker Playback Volume"
        | "Headphone Playback Volume" => {
            kind == ControlKind::Integer
                && !values.is_empty()
                && db.is_some_and(|db| {
                    db.len() == values.len() && db.iter().all(|value| *value == 0)
                })
        }
        "PCM Playback Switch"
        | "Master Playback Switch"
        | "Speaker Playback Switch"
        | "Headphone Playback Switch" => {
            kind == ControlKind::Boolean
                && !values.is_empty()
                && values.iter().all(|value| *value == 1)
        }
        _ => false,
    }
}
