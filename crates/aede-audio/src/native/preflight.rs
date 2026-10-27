//! Bounded header admission before foreign decoder allocation.
use super::{Error, Result};
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
};
fn word(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or_else(|| Error("truncated codec header".into()))?
            .try_into()
            .map_err(|_| Error("invalid codec word".into()))?,
    ))
}
pub(super) fn wavpack(file: &mut File) -> Result<()> {
    let length = file.metadata()?.len();
    let mut at = 0u64;
    let mut frames = 0u64;
    let mut blocks = 0u32;
    let mut sample_rate = None;
    let mut stream_clock = None;
    let mut pcm_format = None;
    while at < length {
        if blocks >= 65536 {
            return Err(Error("WavPack header budget exceeded".into()));
        }
        blocks += 1;
        file.seek(SeekFrom::Start(at))?;
        let mut h = [0; 32];
        file.read_exact(&mut h)?;
        if &h[..4] != b"wvpk" {
            if h.starts_with(b"APETAGEX") || h.starts_with(b"TAG") {
                break;
            }
            return Err(Error("invalid WavPack block signature".into()));
        }
        let size = u64::from(word(&h, 4)?) + 8;
        let end = at
            .checked_add(size)
            .filter(|n| (32..=16 * 1024 * 1024).contains(&size) && *n <= length)
            .ok_or_else(|| Error("invalid WavPack block extent".into()))?;
        let count = u64::from(word(&h, 20)?);
        let flags = word(&h, 24)?;
        if h[10] != 0 || h[11] != 0 || count > 524288 || flags & 0x80000000 != 0 {
            return Err(Error("WavPack profile exceeds native budget".into()));
        }
        stream_clock = super::wavpack_metadata::sample_rate(
            file,
            end,
            (count > 0).then_some(flags),
            stream_clock,
        )?;
        if count > 0 {
            let rate = stream_clock.ok_or_else(|| Error("missing WavPack audio clock".into()))?;
            if sample_rate
                .replace(rate)
                .is_some_and(|previous| previous != rate)
            {
                return Err(Error("WavPack sample rate changes between blocks".into()));
            }
            let precision = ((flags & 3) + 1) * 8;
            let precision = precision
                .checked_sub((flags >> 13) & 31)
                .filter(|bits| *bits > 0)
                .ok_or_else(|| Error("invalid WavPack PCM precision".into()))?;
            // Floating representation, channel stride and integer scale are
            // captured once by the foreign adapter; they must stay consistent.
            let format = (flags & (4 | 0x80), precision);
            if pcm_format
                .replace(format)
                .is_some_and(|previous| previous != format)
            {
                return Err(Error(
                    "WavPack PCM representation changes between blocks".into(),
                ));
            }
            if flags & 0x1800 != 0x1800 || u64::from(word(&h, 16)?) != frames {
                return Err(Error(
                    "WavPack requires contiguous mono/stereo blocks".into(),
                ));
            }
            frames = frames
                .checked_add(count)
                .ok_or_else(|| Error("WavPack count overflow".into()))?;
            let total = word(&h, 12)?;
            if total != 0 && total != u32::MAX && frames > u64::from(total) {
                return Err(Error("inconsistent WavPack total".into()));
            }
        }
        at = end;
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}
fn varint(file: &mut File) -> Result<u64> {
    let mut n = 0u64;
    for _ in 0..8 {
        let mut b = [0];
        file.read_exact(&mut b)?;
        n = n
            .checked_mul(128)
            .and_then(|n| n.checked_add(u64::from(b[0] & 127)))
            .ok_or_else(|| Error("Musepack varint overflow".into()))?;
        if b[0] & 128 == 0 {
            return Ok(n);
        }
    }
    Err(Error("Musepack varint too long".into()))
}
pub(super) fn musepack(file: &mut File) -> Result<()> {
    let length = file.metadata()?.len();
    if length > i32::MAX as u64 {
        return Err(Error("Musepack reader supports files below 2 GiB".into()));
    }
    let mut h = [0; 8];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut h)?;
    const MAX_SAMPLES: u64 = 48000 * 24 * 60 * 60;
    if &h[..3] == b"MP+" {
        let frames = word(&h, 4)?;
        // The legacy parser subtracts synthesis delay from an unsigned count
        // before our output-property validation can run.
        if h[3] & 15 != 7 || frames == 0 || u64::from(frames) * 1152 > MAX_SAMPLES {
            return Err(Error("unsupported Musepack SV7 header".into()));
        }
    } else {
        file.seek(SeekFrom::Start(4))?;
        let mut at = 4u64;
        let mut saw_header = false;
        let mut ended = false;
        let mut block_power = 0u8;
        let mut packet_starts = BTreeSet::new();
        let mut seek_targets = Vec::new();
        for _ in 0..65536 {
            if at >= length {
                break;
            }
            let mut key = [0; 2];
            file.read_exact(&mut key)?;
            let size = varint(file)?;
            let body = file.stream_position()?;
            let end = at
                .checked_add(size)
                .filter(|n| *n >= body && *n <= length && size <= 16 * 1024 * 1024)
                .ok_or_else(|| Error("invalid Musepack packet extent".into()))?;
            packet_starts.insert(at);
            if saw_header && &key == b"SH" {
                return Err(Error("duplicate Musepack stream header".into()));
            }
            if !saw_header {
                if &key != b"SH" || end - body > 256 {
                    return Err(Error("missing/big Musepack stream header".into()));
                }
                let mut prefix = [0; 5];
                file.read_exact(&mut prefix)?;
                if prefix[4] != 8 || varint(file)? > MAX_SAMPLES {
                    return Err(Error("unsupported Musepack sample count".into()));
                }
                let _silence = varint(file)?;
                let mut layout = [0; 2];
                file.read_exact(&mut layout)?;
                if layout[1] >> 4 > 1 || file.stream_position()? > end {
                    return Err(Error("unsupported Musepack layout".into()));
                }
                block_power = (layout[1] & 7) * 2;
                saw_header = true;
            }
            if &key == b"SO" {
                let offset = varint(file)?;
                let position = file.stream_position()?;
                let target = at
                    .checked_add(offset)
                    .filter(|n| *n < length && position <= end)
                    .ok_or_else(|| Error("invalid Musepack seek reference".into()))?;
                seek_targets.push(target);
            }
            if &key == b"ST" {
                super::musepack_seek::validate(file, end, block_power, length)?;
            }
            if &key == b"SE" {
                ended = true;
                break;
            }
            at = end;
            file.seek(SeekFrom::Start(at))?;
        }
        if !saw_header || !ended {
            return Err(Error("incomplete Musepack packet stream".into()));
        }
        if seek_targets.iter().any(|at| !packet_starts.contains(at)) {
            return Err(Error(
                "Musepack seek reference is not a packet boundary".into(),
            ));
        }
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}
