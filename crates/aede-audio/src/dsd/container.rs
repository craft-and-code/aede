//! Bounded DSF (Sony 1.01) and uncompressed DSDIFF (Philips 1.5) extents.
use super::{DsdError, Info};
use std::io::{Read, Seek, SeekFrom};

pub(super) struct Layout {
    pub info: Info,
    pub offset: u64,
    pub block: usize,
    pub lsb: bool,
}

fn error(text: &str) -> DsdError {
    DsdError(text.into())
}
fn bytes<const N: usize>(file: &mut (impl Read + Seek), at: u64) -> Result<[u8; N], DsdError> {
    file.seek(SeekFrom::Start(at))?;
    let mut value = [0; N];
    file.read_exact(&mut value)?;
    Ok(value)
}
fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(std::array::from_fn(|i| b[at + i]))
}
fn be64(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(std::array::from_fn(|i| b[at + i]))
}

pub(super) fn parse(file: &mut (impl Read + Seek), length: u64) -> Result<Layout, DsdError> {
    match &bytes::<4>(file, 0)? {
        b"DSD " => {
            let layout = dsf(file, length)?;
            validate(
                layout.info.sample_rate,
                layout.info.channels,
                layout.info.samples,
            )?;
            Ok(layout)
        }
        b"FRM8" => dff(file, length),
        _ => Err(error("not a DSF/DFF source")),
    }
}
fn validate(rate: u32, channels: usize, samples: u64) -> Result<(), DsdError> {
    if !super::supports_pcm_profile(rate, channels) {
        return Err(error("native DSD supports DSD64/128 mono/stereo only"));
    }
    if samples == 0 {
        return Err(error("DSD source contains no samples"));
    }
    Ok(())
}
pub(super) fn dsf(file: &mut (impl Read + Seek), length: u64) -> Result<Layout, DsdError> {
    let head = bytes::<92>(file, 0)?;
    if &head[..4] != b"DSD "
        || le64(&head, 4) != 28
        || le64(&head, 12) != length
        || &head[28..32] != b"fmt "
        || le64(&head, 32) != 52
        || le32(&head, 40) != 1
        || le32(&head, 44) != 0
        || le32(&head, 76) != 0
        || &head[80..84] != b"data"
    {
        return Err(error("invalid DSF header"));
    }
    let rate = le32(&head, 56);
    let channels = le32(&head, 52) as usize;
    let samples = le64(&head, 64);
    if rate == 0 || samples == 0 {
        return Err(error("DSF source has no sample rate or samples"));
    }
    // Sony DSF 1.01 channel types: mono, stereo, 3ch, quad, 4ch,
    // 5ch and 5.1. Structural inspection does not enable native decoding.
    let expected_channels = match le32(&head, 48) {
        1 => 1,
        2 => 2,
        3 => 3,
        4 | 5 => 4,
        6 => 5,
        7 => 6,
        _ => return Err(error("invalid DSF channel type")),
    };
    if channels != expected_channels
        || le32(&head, 72) != 4096
        || ![1, 8].contains(&le32(&head, 60))
    {
        return Err(error(
            "unsupported DSF channel order, block size or bit order",
        ));
    }
    let payload = samples
        .div_ceil(8)
        .div_ceil(4096)
        .checked_mul(4096 * channels as u64)
        .ok_or_else(|| error("DSF size overflow"))?;
    let end = 92u64
        .checked_add(payload)
        .filter(|end| *end <= length)
        .ok_or_else(|| error("truncated DSF audio"))?;
    if le64(&head, 84) != payload + 12 {
        return Err(error("invalid DSF data size"));
    }
    let metadata = le64(&head, 20);
    if (metadata == 0 && end != length)
        || (metadata != 0 && (metadata != end || metadata >= length))
    {
        return Err(error("invalid DSF metadata extent"));
    }
    Ok(Layout {
        info: Info {
            sample_rate: rate,
            channels,
            samples,
            container: "DSF",
            metadata_offset: (metadata != 0).then_some(metadata),
            metadata_bytes: length - end,
        },
        offset: 92,
        block: 4096,
        lsb: le32(&head, 60) == 1,
    })
}

struct Chunk {
    id: [u8; 4],
    body: u64,
    end: u64,
    next: u64,
    size: u64,
}
fn chunk(file: &mut (impl Read + Seek), at: u64, limit: u64) -> Result<Chunk, DsdError> {
    let body = at
        .checked_add(12)
        .filter(|v| *v <= limit)
        .ok_or_else(|| error("truncated DFF chunk"))?;
    let head = bytes::<12>(file, at)?;
    let size = be64(&head, 4);
    let end = body
        .checked_add(size)
        .ok_or_else(|| error("DFF size overflow"))?;
    let next = end
        .checked_add(size & 1)
        .filter(|v| *v <= limit)
        .ok_or_else(|| error("truncated DFF body"))?;
    Ok(Chunk {
        id: [head[0], head[1], head[2], head[3]],
        body,
        end,
        next,
        size,
    })
}
fn properties(
    file: &mut (impl Read + Seek),
    prop: &Chunk,
    count: &mut usize,
) -> Result<(u32, usize), DsdError> {
    if prop.size < 4 || &bytes::<4>(file, prop.body)? != b"SND " {
        return Err(error("invalid DFF sound properties"));
    }
    let (mut rate, mut channels, mut compression) = (None, None, false);
    let mut at = prop.body + 4;
    while at < prop.end {
        budget(count)?;
        let item = chunk(file, at, prop.end)?;
        match &item.id {
            b"FS  " if rate.is_none() && item.size == 4 => {
                rate = Some(u32::from_be_bytes(bytes::<4>(file, item.body)?))
            }
            b"CHNL" if channels.is_none() && item.size >= 6 => {
                let n = usize::from(u16::from_be_bytes(bytes::<2>(file, item.body)?));
                if !(1..=2).contains(&n) || item.size != 2 + 4 * n as u64 {
                    return Err(error("unsupported DFF channel count"));
                }
                for i in 0..n {
                    let id = bytes::<4>(file, item.body + 2 + 4 * i as u64)?;
                    if (n == 2 && id != *[b"SLFT", b"SRGT"][i])
                        || (n == 1 && id != *b"C   " && id != *b"SLFT")
                    {
                        return Err(error("unsupported DFF channel order"));
                    }
                }
                channels = Some(n);
            }
            b"CMPR" if !compression && item.size >= 5 => {
                let codec = bytes::<5>(file, item.body)?;
                if item.size != 5 + u64::from(codec[4]) {
                    return Err(error("invalid DFF compression name"));
                }
                if &codec[..4] != b"DSD " {
                    return Err(error("DST/compressed DFF is unsupported"));
                }
                compression = true;
            }
            b"FS  " | b"CHNL" | b"CMPR" => return Err(error("invalid or duplicate DFF property")),
            _ => {}
        }
        at = item.next;
    }
    if !compression {
        return Err(error("missing DFF compression"));
    }
    Ok((
        rate.ok_or_else(|| error("missing DFF sample rate"))?,
        channels.ok_or_else(|| error("missing DFF channels"))?,
    ))
}
fn budget(count: &mut usize) -> Result<(), DsdError> {
    *count += 1;
    if *count > 65_536 {
        return Err(error("DSD header count exceeds budget"));
    }
    Ok(())
}
fn dff(file: &mut (impl Read + Seek), length: u64) -> Result<Layout, DsdError> {
    file.seek(SeekFrom::Start(0))?;
    if super::read_dff_header(file)? != length {
        return Err(error("invalid DFF form size/type"));
    }
    let (mut props, mut data) = (None, None);
    let mut id3 = None;
    let mut at = 32;
    let mut count = 1; // The required format-version chunk was already read.
    while at < length {
        budget(&mut count)?;
        let item = chunk(file, at, length)?;
        match &item.id {
            b"PROP" if props.is_none() => props = Some(properties(file, &item, &mut count)?),
            b"DSD " if data.is_none() && props.is_some() => data = Some((item.body, item.size)),
            b"ID3 " if id3.is_none() => id3 = Some((item.body, item.size)),
            b"ID3 " => return Err(error("duplicate DFF ID3 metadata")),
            b"DST " => return Err(error("DST/compressed DFF is unsupported")),
            b"FVER" | b"PROP" | b"DSD " => return Err(error("duplicate or invalid DFF chunk")),
            _ => {}
        }
        at = item.next;
    }
    let (rate, channels) = props.ok_or_else(|| error("missing DFF properties"))?;
    let (offset, size) = data.ok_or_else(|| error("missing DFF audio"))?;
    if size % channels as u64 != 0 {
        return Err(error("incomplete DFF channel frame"));
    }
    let samples = (size / channels as u64)
        .checked_mul(8)
        .ok_or_else(|| error("DFF sample count overflow"))?;
    validate(rate, channels, samples)?;
    Ok(Layout {
        info: Info {
            sample_rate: rate,
            channels,
            samples,
            container: "DFF",
            metadata_offset: id3.map(|(offset, _)| offset),
            metadata_bytes: id3.map_or(0, |(_, size)| size),
        },
        offset,
        block: 1,
        lsb: false,
    })
}
