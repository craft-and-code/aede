//! Small seek-only ISO-BMFF header view; never load artwork or encoded audio.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};
use symphonia::core::errors::{Error, Result};

#[derive(Clone, Copy)]
pub(super) struct Atom {
    pub kind: [u8; 4],
    pub body: u64,
    pub end: u64,
}
pub(super) struct Reader<'a> {
    pub file: &'a mut File,
    pub length: u64,
    remaining: usize,
}
impl<'a> Reader<'a> {
    pub fn new(file: &'a mut File) -> Result<Self> {
        let length = file.metadata()?.len();
        Ok(Self {
            file,
            length,
            remaining: 65536,
        })
    }
    pub fn children(&mut self, start: u64, end: u64) -> Result<Vec<Atom>> {
        let mut at = start;
        let mut atoms = Vec::new();
        while at < end {
            if self.remaining == 0 || end - at < 8 {
                return Err(Error::DecodeError("invalid or excessive MP4 boxes"));
            }
            self.remaining -= 1;
            let head = self.bytes(at, 8)?;
            let mut size = u64::from(word(&head, 0)?);
            let mut body = at
                .checked_add(8)
                .ok_or(Error::DecodeError("MP4 extent overflow"))?;
            if size == 1 {
                if end - at < 16 {
                    return Err(Error::DecodeError("truncated extended MP4 box"));
                }
                size = wide(&self.bytes(body, 8)?, 0)?;
                body += 8;
            } else if size == 0 {
                size = end - at;
            }
            let limit = at
                .checked_add(size)
                .filter(|n| *n >= body && *n <= end && *n <= self.length)
                .ok_or(Error::DecodeError("invalid MP4 box extent"))?;
            let kind = head
                .get(4..8)
                .ok_or(Error::DecodeError("truncated MP4 kind"))?
                .try_into()
                .map_err(|_| Error::DecodeError("truncated MP4 kind"))?;
            atoms.push(Atom {
                kind,
                body,
                end: limit,
            });
            at = limit;
        }
        Ok(atoms)
    }
    pub fn one(&mut self, parent: Atom, kind: [u8; 4]) -> Result<Atom> {
        let mut found = None;
        for atom in self.children(parent.body, parent.end)? {
            if atom.kind == kind && found.replace(atom).is_some() {
                return Err(Error::Unsupported("duplicate AAC MP4 header"));
            }
        }
        found.ok_or(Error::Unsupported("missing AAC MP4 timing header"))
    }
    pub fn bytes(&mut self, at: u64, length: usize) -> Result<Vec<u8>> {
        if length > 32776
            || at
                .checked_add(length as u64)
                .is_none_or(|n| n > self.length)
        {
            return Err(Error::DecodeError(
                "MP4 header exceeds its source or budget",
            ));
        }
        let mut bytes = vec![0; length];
        self.file.seek(SeekFrom::Start(at))?;
        self.file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
    pub fn payload(&mut self, atom: Atom, length: usize) -> Result<Vec<u8>> {
        if length as u64 > atom.end - atom.body {
            return Err(Error::DecodeError("truncated AAC MP4 header"));
        }
        self.bytes(atom.body, length)
    }
}
pub(super) fn word(data: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        data.get(at..at + 4)
            .ok_or(Error::DecodeError("truncated MP4 word"))?
            .try_into()
            .map_err(|_| Error::DecodeError("truncated MP4 word"))?,
    ))
}
pub(super) fn wide(data: &[u8], at: usize) -> Result<u64> {
    Ok(u64::from_be_bytes(
        data.get(at..at + 8)
            .ok_or(Error::DecodeError("truncated MP4 wide word"))?
            .try_into()
            .map_err(|_| Error::DecodeError("truncated MP4 wide word"))?,
    ))
}
