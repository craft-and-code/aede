//! Bounded WavPack metadata framing and effective sample clocks. Field IDs,
//! word lengths and odd/large flags follow the pinned WavPack public header.
use super::{Error, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub(super) fn sample_rate(
    file: &mut File,
    end: u64,
    flags: Option<u32>,
    previous: Option<u32>,
) -> Result<Option<u32>> {
    const RATES: [u32; 15] = [
        6000, 8000, 9600, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200,
        96000, 192000,
    ];
    let header = flags.and_then(|flags| RATES.get(((flags >> 23) & 15) as usize).copied());
    let mut custom = None;
    let mut at = file.stream_position()?;
    let mut count = 0u32;
    while at < end {
        if count >= 65536 || end - at < 2 {
            return Err(Error("invalid WavPack metadata header/budget".into()));
        }
        count += 1;
        file.seek(SeekFrom::Start(at))?;
        let mut field = [0; 2];
        file.read_exact(&mut field)?;
        let mut words = u32::from(field[1]);
        let mut start = at + 2;
        if field[0] & 0x80 != 0 {
            if end - start < 2 {
                return Err(Error("truncated WavPack large metadata header".into()));
            }
            let mut extra = [0; 2];
            file.read_exact(&mut extra)?;
            words |= u32::from(extra[0]) << 8 | u32::from(extra[1]) << 16;
            start += 2;
        }
        let stored = u64::from(words) * 2;
        let next = start
            .checked_add(stored)
            .filter(|value| *value <= end)
            .ok_or_else(|| Error("WavPack metadata exceeds its block".into()))?;
        let length = stored
            .checked_sub(u64::from(field[0] & 0x40 != 0))
            .ok_or_else(|| Error("invalid WavPack odd metadata length".into()))?;
        if field[0] & 0x3f == 0x27 {
            if !matches!(length, 3 | 4) {
                return Err(Error("invalid WavPack sample-rate metadata length".into()));
            }
            let mut bytes = [0; 4];
            file.read_exact(&mut bytes[..length as usize])?;
            let rate = u32::from_le_bytes(bytes) & 0x7fff_ffff;
            if rate == 0 || rate > 768000 || custom.replace(rate).is_some_and(|old| old != rate) {
                return Err(Error("inconsistent WavPack sample-rate metadata".into()));
            }
        }
        at = next;
    }
    if flags.is_none() {
        return Ok(custom.or(previous));
    }
    if header
        .zip(custom.or(previous))
        .is_some_and(|(standard, explicit)| standard != explicit)
    {
        return Err(Error("WavPack header and metadata clocks disagree".into()));
    }
    // Custom-rate headers may omit a repeated metadata value; metadata-only
    // blocks can establish it before audio. Match the initial 44.1-kHz default.
    Ok(Some(custom.or(header).or(previous).unwrap_or(44100)))
}
