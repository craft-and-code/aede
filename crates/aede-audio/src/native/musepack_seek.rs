//! Bounded SV8 ST admission, from the Musepack SV8 format specification:
//! <https://mutagen-specs.readthedocs.io/en/latest/mpc/sv8.html#seek-table-packet>
//! The legacy C reader has no packet-end bound for these variable-length fields.
use super::{Error, Result};
use std::{
    fs::File,
    io::{BufReader, Read, Seek},
};

pub(super) fn validate(file: &mut File, end: u64, block_power: u8, length: u64) -> Result<()> {
    let bytes = end
        .checked_sub(file.stream_position()?)
        .filter(|n| *n <= 65536)
        .ok_or_else(|| Error("Musepack seek packet exceeds native budget".into()))?;
    table(
        BufReader::with_capacity(4096, file.take(bytes)),
        block_power,
        length,
    )
}

fn table(input: impl Read, block_power: u8, length: u64) -> Result<()> {
    let mut bits = Bits {
        input,
        byte: 0,
        left: 0,
    };
    let count = bits.varint()?;
    if !(1..=65536).contains(&count) {
        return Err(Error(
            "Musepack seek entry count exceeds native budget".into(),
        ));
    }
    if u32::from(block_power) + bits.read(4)? > 20 {
        return Err(Error(
            "Musepack seek exponent exceeds the native word".into(),
        ));
    }
    let mut last = [0i128; 2];
    for i in 0..count {
        let slot = (i & 1) as usize;
        let position = if i < 2 {
            i128::from(bits.varint()?)
        } else {
            let code = bits.golomb()?;
            let delta = i128::from(code / 2);
            2 * last[1 - slot] - last[slot] + if code & 1 == 0 { delta } else { -delta }
        };
        if position < 0 || position >= i128::from(length) {
            return Err(Error("Musepack seek position is outside source".into()));
        }
        last[slot] = position;
    }
    bits.finish()
}

struct Bits<R> {
    input: R,
    byte: u8,
    left: u8,
}
impl<R: Read> Bits<R> {
    fn read(&mut self, count: u8) -> Result<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            if self.left == 0 {
                let mut byte = [0];
                self.input
                    .read_exact(&mut byte)
                    .map_err(|e| Error(format!("truncated Musepack seek data: {e}")))?;
                self.byte = byte[0];
                self.left = 8;
            }
            self.left -= 1;
            value = (value << 1) | u32::from((self.byte >> self.left) & 1);
        }
        Ok(value)
    }
    fn varint(&mut self) -> Result<u64> {
        let mut value = 0u64;
        for _ in 0..8 {
            let byte = self.read(8)?;
            value = (value << 7) | u64::from(byte & 127);
            if byte & 128 == 0 {
                return Ok(value);
            }
        }
        Err(Error("Musepack seek varint exceeds native word".into()))
    }
    fn golomb(&mut self) -> Result<u32> {
        let mut quotient = 0u32;
        while self.read(1)? == 0 {
            quotient += 1;
            if quotient > (i32::MAX as u32 >> 12) {
                return Err(Error("Musepack seek residual exceeds native word".into()));
            }
        }
        Ok((quotient << 12) | self.read(12)?)
    }
    fn finish(mut self) -> Result<()> {
        if u16::from(self.byte) & ((1u16 << self.left) - 1) != 0 {
            return Err(Error("nonzero Musepack seek padding".into()));
        }
        let mut padding = [0; 4096];
        loop {
            let count = self.input.read(&mut padding)?;
            if count == 0 {
                return Ok(());
            }
            if padding[..count].iter().any(|b| *b != 0) {
                return Err(Error("nonzero Musepack seek padding".into()));
            }
        }
    }
}

#[cfg(test)]
#[path = "musepack_seek_tests.rs"]
mod tests;
