//! Skip validated trailing tags by extent, never by loading their payloads.
use super::{Error, Result, Source, word};
use std::io::{Read, Seek, SeekFrom};
pub(super) fn audio_file_end(file: &mut Source) -> Result<u64> {
    let mut end = file.length;
    for _ in 0..8 {
        if end >= 128 {
            file.seek(SeekFrom::Start(end - 128))?;
            let mut magic = [0; 3];
            file.read_exact(&mut magic)?;
            if &magic == b"TAG" {
                end -= 128;
                continue;
            }
        }
        if end < 32 {
            return Ok(end);
        }
        file.seek(SeekFrom::Start(end - 32))?;
        let mut footer = [0; 32];
        file.read_exact(&mut footer)?;
        if &footer[..8] != b"APETAGEX" {
            return Ok(end);
        }
        let version = word(&footer, 8)?;
        let size = u64::from(word(&footer, 12)?);
        let fields = word(&footer, 16)?;
        let flags = word(&footer, 20)?;
        if ![1000, 2000].contains(&version)
            || size < 32
            || size > end
            || fields > 65536
            || footer[24..] != [0; 8]
            || flags & (1 << 29) != 0
        {
            return Err(Error("invalid trailing APE tag extent".into()));
        }
        end -= size;
        if flags & (1 << 31) != 0 {
            end = end
                .checked_sub(32)
                .ok_or_else(|| Error("truncated APE tag header".into()))?;
            file.seek(SeekFrom::Start(end))?;
            let mut header = [0; 32];
            file.read_exact(&mut header)?;
            if &header[..8] != b"APETAGEX"
                || header[8..20] != footer[8..20]
                || word(&header, 20)? != (flags | (1 << 29))
                || header[24..] != [0; 8]
            {
                return Err(Error("inconsistent APE tag header/footer".into()));
            }
        }
    }
    Err(Error("too many trailing APE tags".into()))
}
