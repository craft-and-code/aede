//! Bounded header for the self-describing float WAV emitted by the fallback decoder.
use crate::decode::DecodeError as Error;
use std::io::Read;
fn invalid() -> Error {
    Error("invalid external float WAV header".into())
}
fn read<const N: usize>(reader: &mut impl Read) -> Result<[u8; N], Error> {
    let mut bytes = [0; N];
    reader
        .read_exact(&mut bytes)
        .map_err(|e| Error(e.to_string()))?;
    Ok(bytes)
}
/// Read the bounded float WAV header emitted by a streaming external decoder.
/// Return its actual rate/channel count, leaving the reader at the first PCM byte.
/// Only little-endian IEEE float32 RIFF/WAVE is admitted, including its extensible
/// form. This is not a general-purpose WAV file reader or a payload validator.
pub fn read_format(reader: &mut impl Read) -> Result<(u32, u16), Error> {
    let header = read::<12>(reader)?;
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return Err(invalid());
    }
    let mut format = None;
    let mut budget = 65_536u64;
    for _ in 0..64 {
        let head = read::<8>(reader)?;
        let length = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as u64;
        if &head[..4] == b"data" {
            return format.ok_or_else(invalid);
        }
        budget = budget
            .checked_sub(8 + length + (length & 1))
            .ok_or_else(invalid)?;
        if &head[..4] == b"fmt " {
            if format.is_some() || !(16..=64).contains(&length) {
                return Err(invalid());
            }
            let mut bytes = [0; 64];
            reader
                .read_exact(&mut bytes[..length as usize])
                .map_err(|e| Error(e.to_string()))?;
            let tag = u16::from_le_bytes([bytes[0], bytes[1]]);
            let channels = u16::from_le_bytes([bytes[2], bytes[3]]);
            let rate = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let bytes_per_second = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            let align = u16::from_le_bytes([bytes[12], bytes[13]]);
            let bits = u16::from_le_bytes([bytes[14], bytes[15]]);
            let float = tag == 3
                || (tag == 0xfffe
                    && length >= 40
                    && bytes[16..20] == [22, 0, 32, 0]
                    && bytes[24..40] == [3, 0, 0, 0, 0, 0, 16, 0, 128, 0, 0, 170, 0, 56, 155, 113]);
            if !float
                || !(1..=32).contains(&channels)
                || rate == 0
                || bits != 32
                || align != channels * 4
                || rate.checked_mul(u32::from(align)) != Some(bytes_per_second)
            {
                return Err(invalid());
            }
            format = Some((rate, channels));
        } else {
            let copied = std::io::copy(&mut (&mut *reader).take(length), &mut std::io::sink())
                .map_err(|e| Error(e.to_string()))?;
            if copied != length {
                return Err(invalid());
            }
        }
        if length & 1 != 0 {
            read::<1>(reader)?;
        }
    }
    Err(invalid())
}
#[cfg(test)]
#[path = "pcm_wave_tests.rs"]
mod tests;
