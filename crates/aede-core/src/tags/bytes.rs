//! Small binary reading utilities, shared by every parser.
//!
//! Everything is fallible and bounded: a truncated file must produce an error,
//! never a panic. No `unwrap` and no direct indexing here.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use super::TagError;

/// Read-only cursor over a byte buffer.
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

/// Not every primitive is used by every format; together they form a coherent
/// set that future parsers (DSF, WavPack…) will draw on.
#[allow(dead_code)]
impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub fn skip(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.data.len());
    }

    pub fn seek_to(&mut self, pos: usize) {
        self.pos = pos.min(self.data.len());
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.remaining() < n {
            return None;
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Some(slice)
    }

    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }

    pub fn u16_be(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_be_bytes([s[0], s[1]]))
    }

    pub fn u24_be(&mut self) -> Option<u32> {
        self.take(3)
            .map(|s| ((s[0] as u32) << 16) | ((s[1] as u32) << 8) | s[2] as u32)
    }

    pub fn u32_be(&mut self) -> Option<u32> {
        self.take(4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }

    pub fn u64_be(&mut self) -> Option<u64> {
        self.take(8)
            .map(|s| u64::from_be_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
    }

    pub fn u16_le(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }

    pub fn u32_le(&mut self) -> Option<u32> {
        self.take(4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    pub fn u64_le(&mut self) -> Option<u64> {
        self.take(8)
            .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
    }

    /// Reads `n` bytes and interprets them as UTF-8, replacing invalid
    /// sequences rather than failing: a slightly damaged name is better than a
    /// skipped file.
    pub fn utf8(&mut self, n: usize) -> Option<String> {
        self.take(n)
            .map(|s| String::from_utf8_lossy(s).into_owned())
    }

    /// Checks a magic signature without consuming it when it does not match.
    pub fn expect_magic(&mut self, magic: &[u8]) -> bool {
        if self.remaining() < magic.len() {
            return false;
        }
        if &self.data[self.pos..self.pos + magic.len()] == magic {
            self.pos += magic.len();
            true
        } else {
            false
        }
    }
}

/// Reads exactly `len` bytes at offset `offset`.
pub fn read_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, TagError> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

/// Reads up to `len` bytes at offset `offset`, without failing if the file is
/// shorter.
pub fn read_at_most(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, TagError> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; len];
    let mut total = 0;
    while total < len {
        match file.read(&mut buf[total..])? {
            0 => break,
            n => total += n,
        }
    }
    buf.truncate(total);
    Ok(buf)
}

/// ID3v2 "syncsafe" integer: 4 bytes carrying 7 useful bits each.
pub fn syncsafe(bytes: &[u8]) -> u32 {
    let mut value = 0u32;
    for &b in bytes.iter().take(4) {
        value = (value << 7) | (b & 0x7F) as u32;
    }
    value
}

/// Converts an 80-bit "extended" float (used by AIFF) into an `f64`.
pub fn extended80_to_f64(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 10 {
        return None;
    }
    let sign = if bytes[0] & 0x80 != 0 { -1.0 } else { 1.0 };
    let exponent = (((bytes[0] & 0x7F) as i32) << 8) | bytes[1] as i32;
    let mut mantissa = 0u64;
    for &b in &bytes[2..10] {
        mantissa = (mantissa << 8) | b as u64;
    }
    if exponent == 0 && mantissa == 0 {
        return Some(0.0);
    }
    if exponent == 0x7FFF {
        return None; // infinity or NaN: unusable value
    }
    Some(sign * (mantissa as f64) * 2f64.powi(exponent - 16383 - 63))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_bounds_reads() {
        let data = [1u8, 2, 3];
        let mut c = Cursor::new(&data);
        assert_eq!(c.u16_be(), Some(0x0102));
        assert_eq!(c.remaining(), 1);
        assert_eq!(c.u32_be(), None, "an out-of-bounds read returns None");
        assert_eq!(c.remaining(), 1, "and consumes nothing");
    }

    #[test]
    fn syncsafe_id3() {
        // 0x00 0x00 0x02 0x01 => 257
        assert_eq!(syncsafe(&[0x00, 0x00, 0x02, 0x01]), 257);
        assert_eq!(syncsafe(&[0x00, 0x00, 0x00, 0x7F]), 127);
        assert_eq!(syncsafe(&[0x00, 0x00, 0x01, 0x00]), 128);
    }

    #[test]
    fn extended80() {
        // 44100 Hz encoded as an 80-bit extended float.
        let bytes = [0x40, 0x0E, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let value = extended80_to_f64(&bytes).unwrap();
        assert!((value - 44100.0).abs() < 0.001, "got {value}");
    }

    #[test]
    fn magic_does_not_consume_when_absent() {
        let data = b"fLaC";
        let mut c = Cursor::new(data);
        assert!(!c.expect_magic(b"OggS"));
        assert_eq!(c.position(), 0);
        assert!(c.expect_magic(b"fLaC"));
        assert_eq!(c.position(), 4);
    }
}
