//! Explicit direct-hardware selectors; no plugin or default substitutions.
use std::ffi::CString;
/// Parsed explicit card/device/subdevice identity, before hardware resolution.
pub struct Selection {
    pub card: String,
    pub device: u32,
    pub subdevice: u32,
}

fn number(value: &str) -> Result<u32, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("ALSA device/subdevice index must be an unsigned decimal integer".to_owned());
    }
    value
        .parse()
        .map_err(|_| "ALSA device/subdevice index exceeds u32".to_owned())
}

/// Admit only a bounded direct-hardware selector with explicit card and device.
pub fn selection(name: &str) -> Result<Selection, String> {
    if !name.starts_with("hw:") || name.len() > 256 || name[3..].is_empty() {
        return Err("strict ALSA output requires an explicit hw: device ID".to_owned());
    }
    let fields = name[3..].split(',').collect::<Vec<_>>();
    if !(2..=3).contains(&fields.len()) {
        return Err("strict ALSA selection requires CARD and DEV, with optional SUBDEV".to_owned());
    }
    let (card, device, subdevice) = if fields.iter().any(|field| field.contains('=')) {
        let (mut card, mut device, mut subdevice) = (None, None, None);
        for field in &fields {
            match field.split_once('=') {
                Some(("CARD", value)) if card.is_none() => card = Some(value),
                Some(("DEV", value)) if device.is_none() => device = Some(number(value)?),
                Some(("SUBDEV", value)) if subdevice.is_none() => subdevice = Some(number(value)?),
                _ => return Err("ALSA selection has an unknown or duplicated field".to_owned()),
            }
        }
        (
            card.ok_or("ALSA selection is missing CARD")?,
            device.ok_or("ALSA selection is missing DEV")?,
            subdevice.unwrap_or(0),
        )
    } else {
        (
            fields[0],
            number(fields[1])?,
            if fields.len() == 3 {
                number(fields[2])?
            } else {
                0
            },
        )
    };
    if card.is_empty()
        || !card
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(
            "ALSA card ID must be a numeric index or a simple symbolic identifier".to_owned(),
        );
    }
    Ok(Selection {
        card: card.to_owned(),
        device,
        subdevice,
    })
}

/// Match the opened hardware substream against the retained selection.
pub fn matches_identity(
    requested: &Selection,
    resolved_card: i32,
    actual: (i32, u32, u32),
    substreams: u32,
) -> bool {
    resolved_card >= 0
        && substreams == 1
        && actual == (resolved_card, requested.device, requested.subdevice)
}

/// Validate the direct selector before passing its bytes to ALSA.
pub fn hardware_name(name: &str) -> Result<CString, String> {
    selection(name)?;
    CString::new(name).map_err(|_| "ALSA device ID contains a NUL byte".to_owned())
}
