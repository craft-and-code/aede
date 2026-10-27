//! The 32-byte pre-3.98 header has optional fields and no declared audio length.
use super::layout::{self, Layout};
use super::{Error, Result, Source, half, word};
use std::io::{Read, Seek, SeekFrom};
pub(super) fn read(file: &mut Source, version: u16) -> Result<Layout> {
    file.seek(SeekFrom::Start(0))?;
    let mut header = [0; 32];
    file.read_exact(&mut header)?;
    let compression = half(&header, 6)?;
    let flags = half(&header, 8)?;
    if ![1000, 2000, 3000, 4000, 5000].contains(&compression)
        || flags & !0x3f != 0
        || flags & 9 == 9
    {
        return Err(Error("unsupported legacy APE flags/compression".into()));
    }
    let bits = if flags & 1 != 0 {
        8
    } else if flags & 8 != 0 {
        24
    } else {
        16
    };
    let blocks = if version >= 3950 {
        294912
    } else if version >= 3900 || (version >= 3800 && compression == 4000) {
        73728
    } else {
        9216
    };
    let count = word(&header, 24)?;
    let layout = layout::properties(
        version,
        word(&header, 12)?,
        half(&header, 10)?,
        bits,
        blocks,
        word(&header, 28)?,
        count,
    )?;
    if flags & 4 != 0 {
        let mut peak = [0; 4];
        file.read_exact(&mut peak)?;
    }
    let entries = if flags & 16 != 0 {
        let mut b = [0; 4];
        file.read_exact(&mut b)?;
        u32::from_le_bytes(b)
    } else {
        count
    };
    if entries < count || entries > 1024 * 1024 {
        return Err(Error("legacy APE seek table exceeds native budget".into()));
    }
    let wrapper = if flags & 32 != 0 {
        0
    } else {
        u64::from(word(&header, 16)?)
    };
    if wrapper > 1024 * 1024 {
        return Err(Error("legacy APE wrapper exceeds native budget".into()));
    }
    let table = file
        .stream_position()?
        .checked_add(wrapper)
        .ok_or_else(|| Error("APE header overflow".into()))?;
    let bit_table = table
        .checked_add(u64::from(entries) * 4)
        .ok_or_else(|| Error("APE table overflow".into()))?;
    let bits = if version <= 3800 {
        Some(bit_table)
    } else {
        None
    };
    let start = bit_table
        .checked_add(if bits.is_some() {
            u64::from(entries)
        } else {
            0
        })
        .ok_or_else(|| Error("APE table overflow".into()))?;
    let untagged = super::tags::audio_file_end(file)?;
    let end = untagged
        .checked_sub(u64::from(word(&header, 20)?))
        .filter(|end| *end > start)
        .ok_or_else(|| Error("legacy APE data exceeds source".into()))?;
    layout::positions(file, table, count, start, end, bits)?;
    file.length = untagged;
    Ok(layout)
}
