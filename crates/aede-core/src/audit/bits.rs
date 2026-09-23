//! A bit-level reader, because FLAC subframes are not byte aligned.
//!
//! Every read is bounded and returns `Option`, so a truncated or malformed
//! stream stops the walk instead of panicking.

/// Reads big-endian bit fields from a byte slice.
pub struct BitReader<'a> {
    data: &'a [u8],
    /// Position in bits from the start of the slice.
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> BitReader<'a> {
        BitReader { data, pos: 0 }
    }

    pub fn align_to_byte(&mut self) {
        self.pos = self.pos.div_ceil(8) * 8;
    }

    /// Position of the next bit to be read, counted from the start.
    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    pub fn is_exhausted(&self) -> bool {
        self.pos >= self.data.len() * 8
    }

    /// Reads `n` bits (at most 64) as an unsigned value.
    pub fn bits(&mut self, n: u32) -> Option<u64> {
        if n == 0 {
            return Some(0);
        }
        if n > 64 || self.pos + n as usize > self.data.len() * 8 {
            return None;
        }
        let mut value: u64 = 0;
        for _ in 0..n {
            let byte = self.data[self.pos >> 3];
            let bit = (byte >> (7 - (self.pos & 7))) & 1;
            value = (value << 1) | bit as u64;
            self.pos += 1;
        }
        Some(value)
    }

    /// Reads `n` bits as a two's-complement signed value.
    pub fn signed_bits(&mut self, n: u32) -> Option<i64> {
        let raw = self.bits(n)?;
        if n == 0 {
            return Some(0);
        }
        let sign = 1u64 << (n - 1);
        Some(if raw & sign != 0 {
            (raw as i64) - (1i64 << n)
        } else {
            raw as i64
        })
    }

    /// Counts zero bits up to the next one bit, and consumes that one bit.
    ///
    /// This is the unary part of a Rice code. The cap keeps a corrupt stream
    /// from turning into an unbounded loop.
    pub fn unary(&mut self) -> Option<u32> {
        let mut zeros = 0u32;
        loop {
            match self.bits(1)? {
                1 => return Some(zeros),
                _ => {
                    zeros += 1;
                    if zeros > 1_000_000 {
                        return None;
                    }
                }
            }
        }
    }

    /// Skips `n` bits, failing if the stream is shorter than that.
    pub fn skip(&mut self, n: usize) -> Option<()> {
        if self.pos + n > self.data.len() * 8 {
            return None;
        }
        self.pos += n;
        Some(())
    }
}

#[cfg(test)]
#[path = "bits_tests.rs"]
mod tests;
