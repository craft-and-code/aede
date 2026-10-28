//! HAL integer-format admission, separate from device I/O and source bits.
use crate::dop::{Carrier, Format};

pub(super) const SIGNED: u32 = 4;
pub(super) const PACKED: u32 = 8;
pub(super) const HIGH: u32 = 16;
pub(super) const NON_MIXABLE: u32 = 64;

/// Apple AudioStreamBasicDescription (CoreAudioBaseTypes.h).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Description {
    pub rate: f64,
    pub id: u32,
    pub flags: u32,
    pub bytes_per_packet: u32,
    pub frames_per_packet: u32,
    pub bytes_per_frame: u32,
    pub channels: u32,
    pub bits: u32,
    pub reserved: u32,
}

/// Apple AudioStreamRangedDescription and AudioValueRange, same SDK header.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Ranged {
    pub format: Description,
    pub minimum: f64,
    pub maximum: f64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Plan {
    pub physical: Description,
    pub virtual_format: Description,
    pub carrier: Carrier,
}

pub(super) fn carrier(
    format: Description,
    target: Format,
    virtual_format: bool,
) -> Result<Carrier, String> {
    let allowed = SIGNED | PACKED | HIGH | NON_MIXABLE;
    if format.id != u32::from_be_bytes(*b"lpcm")
        || format.rate != f64::from(target.carrier_rate())
        || format.channels as usize != target.channels()
        || format.bits != 24
        || format.reserved != 0
        || format.frames_per_packet != 1
        || format.bytes_per_packet != format.bytes_per_frame
        || format.flags & SIGNED == 0
        || format.flags & !allowed != 0
        || (virtual_format && format.flags & NON_MIXABLE == 0)
    {
        return Err(
            "DoP needs exact-rate, non-mixable interleaved signed 24-bit integer HAL I/O".into(),
        );
    }
    let bytes = format.bytes_per_frame / format.channels;
    if !format.bytes_per_frame.is_multiple_of(format.channels) {
        return Err("DoP HAL frame width is inconsistent".into());
    }
    match bytes {
        3 if format.flags & HIGH == 0 => Ok(Carrier::Packed24Le),
        4 if format.flags & PACKED == 0 && format.flags & HIGH != 0 => Ok(Carrier::High24In32Le),
        4 if format.flags & (PACKED | HIGH) == 0 => Ok(Carrier::Low24In32Le),
        _ => Err("DoP HAL integer storage is unsupported".into()),
    }
}

pub(super) fn plan(
    physical: &[Ranged],
    virtual_formats: &[Ranged],
    target: Format,
) -> Result<Plan, String> {
    for physical in physical {
        let Some(physical) = at_rate(*physical, target) else {
            continue;
        };
        let Ok(encoding) = carrier(physical, target, false) else {
            continue;
        };
        for virtual_format in virtual_formats {
            let Some(virtual_format) = at_rate(*virtual_format, target) else {
                continue;
            };
            if carrier(virtual_format, target, true).ok() == Some(encoding) {
                return Ok(Plan {
                    physical,
                    virtual_format,
                    carrier: encoding,
                });
            }
        }
    }
    Err("the selected DAC has no matching exclusive integer DoP carrier; no PCM fallback is performed".into())
}

fn at_rate(mut range: Ranged, target: Format) -> Option<Description> {
    let rate = f64::from(target.carrier_rate());
    if !range.minimum.is_finite()
        || !range.maximum.is_finite()
        || range.minimum <= 0.0
        || range.minimum > range.maximum
        || rate < range.minimum
        || rate > range.maximum
    {
        return None;
    }
    range.format.rate = rate;
    Some(range.format)
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
