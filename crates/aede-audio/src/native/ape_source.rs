//! Bounded APE-relative file view, excluding prefixes and trailing tags.
use super::{Error, Result};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
};
// The dependency derives its last compressed frame from SeekFrom::End rather
// than the descriptor's audio extent. Hide trailing tags from that calculation
// while retaining the declared terminating data it subtracts from the end.
pub(super) struct Source {
    file: File,
    start: u64,
    pub(super) length: u64,
}
impl Source {
    pub(super) fn open(mut file: File) -> Result<Self> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(Error("APE source is not a regular file".into()));
        }
        let start = prefix(&mut file)?;
        let length = metadata.len().saturating_sub(start);
        Ok(Self {
            file,
            start,
            length,
        })
    }
}
impl Read for Source {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let position = self.file.stream_position()?.saturating_sub(self.start);
        let count = (bytes.len() as u64).min(self.length.saturating_sub(position)) as usize;
        self.file.read(&mut bytes[..count])
    }
}
impl Seek for Source {
    fn seek(&mut self, from: SeekFrom) -> std::io::Result<u64> {
        let position = match from {
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(offset) => {
                i128::from(self.file.stream_position()?) - i128::from(self.start)
                    + i128::from(offset)
            }
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
        };
        if position < 0 || position > i128::from(self.length) {
            return Err(std::io::Error::other(
                "APE seek outside declared source extent",
            ));
        }
        self.file
            .seek(SeekFrom::Start(self.start + position as u64))
            .map(|position| position - self.start)
    }
}
// Scan a bounded prefix without asking the dependency to allocate or interpret
// ID3 metadata. The virtual source keeps every APE offset relative to its magic.
fn prefix(file: &mut File) -> Result<u64> {
    file.seek(SeekFrom::Start(0))?;
    let mut input = BufReader::with_capacity(65536, file);
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    let mut start = 0;
    if &magic[..3] == b"ID3" {
        let mut header = [0; 6];
        input.read_exact(&mut header)?;
        if !(2..=4).contains(&magic[3]) || header[2..].iter().any(|b| b & 0x80 != 0) {
            return Err(Error("invalid APE ID3 prefix".into()));
        }
        start = header[2..]
            .iter()
            .fold(0u64, |n, b| (n << 7) | u64::from(*b))
            + 10
            + if magic[3] == 4 && header[1] & 0x10 != 0 {
                10
            } else {
                0
            };
        if start > 1024 * 1024 {
            return Err(Error("APE prefix exceeds native budget".into()));
        }
        input.seek(SeekFrom::Start(start))?;
        input.read_exact(&mut magic)?;
    }
    for offset in start..=1024 * 1024 {
        if &magic == b"MAC " {
            return Ok(offset);
        }
        magic.rotate_left(1);
        input.read_exact(&mut magic[3..])?;
    }
    Err(Error("APE prefix exceeds native budget".into()))
}
