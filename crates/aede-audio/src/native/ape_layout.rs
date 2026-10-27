//! Bounded APE layouts are checked before either dependency allocates a context.
use super::{Error, Info, Kind, Result, Source, half, word};
use std::io::{Read, Seek, SeekFrom};
pub(super) struct Layout {
    pub version: u16,
    pub info: Info,
    pub count: u32,
    pub blocks: u32,
    pub last: u32,
    pub bits: u32,
}
pub(super) fn read(file: &mut Source) -> Result<Layout> {
    file.seek(SeekFrom::Start(0))?;
    let mut prefix = [0; 6];
    file.read_exact(&mut prefix)?;
    let version = half(&prefix, 4)?;
    if &prefix[..4] != b"MAC " || !(3800..=4100).contains(&version) {
        return Err(Error(
            "unsupported APE version: native profile is 3.80–4.10".into(),
        ));
    }
    if version < 3980 {
        return super::legacy::read(file, version);
    }
    file.seek(SeekFrom::Start(0))?;
    let mut descriptor = [0; 52];
    file.read_exact(&mut descriptor)?;
    let d = u64::from(word(&descriptor, 8)?);
    let h = u64::from(word(&descriptor, 12)?);
    let seek = u64::from(word(&descriptor, 16)?);
    let wrapper = u64::from(word(&descriptor, 20)?);
    let data = u64::from(word(&descriptor, 24)?) + (u64::from(word(&descriptor, 28)?) << 32);
    let ending = u64::from(word(&descriptor, 32)?);
    if !(52..=4096).contains(&d)
        || !(24..=4096).contains(&h)
        || seek > 4 * 1024 * 1024
        || wrapper > 1024 * 1024
    {
        return Err(Error("APE descriptor exceeds the admitted profile".into()));
    }
    let start = d
        .checked_add(h)
        .and_then(|v| v.checked_add(seek))
        .and_then(|v| v.checked_add(wrapper))
        .ok_or_else(|| Error("APE header extent overflow".into()))?;
    let end = start
        .checked_add(data)
        .filter(|v| v.checked_add(ending).is_some_and(|n| n <= file.length))
        .ok_or_else(|| Error("APE data exceeds source".into()))?;
    file.seek(SeekFrom::Start(d))?;
    let mut header = [0; 24];
    file.read_exact(&mut header)?;
    if half(&header, 2)? & (512 | 2048 | 4096) != 0 {
        return Err(Error(
            "APE requires ordinary little-endian integer PCM".into(),
        ));
    }
    let count = word(&header, 12)?;
    if seek != u64::from(count) * 4 {
        return Err(Error("invalid APE seek table length".into()));
    }
    let layout = properties(
        version,
        word(&header, 20)?,
        half(&header, 18)?,
        u32::from(half(&header, 16)?),
        word(&header, 4)?,
        word(&header, 8)?,
        count,
    )?;
    positions(file, d + h, count, start, end, None)?;
    file.length = end + ending;
    Ok(layout)
}
pub(super) fn properties(
    version: u16,
    rate: u32,
    channels: u16,
    bits: u32,
    blocks: u32,
    last: u32,
    count: u32,
) -> Result<Layout> {
    if count == 0
        || count > 1024 * 1024
        || !(1..=2).contains(&channels)
        || ![8, 16, 24, 32].contains(&bits)
        || blocks == 0
        || u64::from(blocks) * u64::from(channels) > 1_048_576
        || last == 0
        || last > blocks
    {
        return Err(Error("APE frame layout exceeds native budget".into()));
    }
    let frames = u64::from(count - 1) * u64::from(blocks) + u64::from(last);
    let info = Info {
        kind: Kind::Ape,
        sample_rate: rate,
        channels: usize::from(channels),
        bits: Some(bits),
        frames: Some(frames),
    };
    info.validate()?;
    Ok(Layout {
        version,
        info,
        count,
        blocks,
        last,
        bits,
    })
}
pub(super) fn positions(
    file: &mut Source,
    table: u64,
    count: u32,
    start: u64,
    end: u64,
    bits: Option<u64>,
) -> Result<()> {
    file.seek(SeekFrom::Start(table))?;
    let mut positions = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let mut b = [0; 4];
        file.read_exact(&mut b)?;
        positions.push(u64::from(u32::from_le_bytes(b)));
    }
    for (index, &at) in positions.iter().enumerate() {
        let next = positions.get(index + 1).copied().unwrap_or(end);
        let bit = if let Some(offset) = bits {
            file.seek(SeekFrom::Start(offset + index as u64))?;
            let mut b = [0];
            file.read_exact(&mut b)?;
            if b[0] > 31 {
                return Err(Error("invalid legacy APE seek bit".into()));
            }
            u64::from(b[0])
        } else {
            0
        };
        if at.saturating_add(bit / 8) < start
            || next <= at
            || next > end
            || next - at > 16 * 1024 * 1024
        {
            return Err(Error("invalid APE seek/frame extent".into()));
        }
    }
    Ok(())
}
