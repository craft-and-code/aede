//! Final integer PCM quantization with non-subtractive TPDF dither.

/// One continuous dither sequence for an integer output stream.
///
/// The generator is local and deterministic; no entropy source, allocation, or
/// lock is used in the output callback. Each input sample consumes two uniform
/// draws, giving a triangular distribution spanning two integer LSBs.
pub struct TpdfQuantizer {
    state: u64,
}

impl Default for TpdfQuantizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TpdfQuantizer {
    pub fn new() -> Self {
        Self {
            state: 0x6a09_e667_f3bc_c909,
        }
    }

    pub fn i8(&mut self, sample: f32) -> i8 {
        self.signed(sample, 8) as i8
    }

    pub fn i16(&mut self, sample: f32) -> i16 {
        self.signed(sample, 16) as i16
    }

    pub fn i24(&mut self, sample: f32) -> i32 {
        self.signed(sample, 24) as i32
    }

    pub fn i32(&mut self, sample: f32) -> i32 {
        self.signed(sample, 32) as i32
    }

    pub fn u8(&mut self, sample: f32) -> u8 {
        self.unsigned(sample, 8) as u8
    }

    pub fn u16(&mut self, sample: f32) -> u16 {
        self.unsigned(sample, 16) as u16
    }

    pub fn u24(&mut self, sample: f32) -> u32 {
        self.unsigned(sample, 24) as u32
    }

    pub fn u32(&mut self, sample: f32) -> u32 {
        self.unsigned(sample, 32) as u32
    }

    fn unsigned(&mut self, sample: f32, bits: u32) -> i64 {
        self.signed(sample, bits) + (1_i64 << (bits - 1))
    }

    fn signed(&mut self, sample: f32, bits: u32) -> i64 {
        let half_range = 1_i64 << (bits - 1);
        if sample <= -1.0 {
            return -half_range;
        }
        if sample >= 1.0 {
            return half_range - 1;
        }
        let scaled = f64::from(sample) * half_range as f64;
        let dither = self.uniform() - self.uniform();
        (scaled + dither)
            .round()
            .clamp(-half_range as f64, (half_range - 1) as f64) as i64
    }

    fn uniform(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        (value >> 11) as f64 * (1.0 / ((1_u64 << 53) as f64))
    }
}

#[cfg(test)]
#[path = "dither_tests.rs"]
mod tests;
