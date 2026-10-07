//! Ordinary output conversion, preserving representable samples without effects.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use aede_dsp::TpdfQuantizer;

pub(super) struct SampleConversion {
    without_effects: bool,
    quantizer: TpdfQuantizer,
    quantized: Arc<AtomicU64>,
}

macro_rules! integer_output {
    ($name:ident, $target:ty, $bits:literal, $bias:expr) => {
        pub(super) fn $name(&mut self, sample: f32) -> $target {
            if self.without_effects
                && let Some(value) = exact_signed(sample, $bits)
            {
                return (value + $bias) as $target;
            }
            self.quantized.fetch_add(1, Ordering::Relaxed);
            self.quantizer.$name(sample)
        }
    };
}

impl SampleConversion {
    pub(super) fn new(without_effects: bool, quantized: Arc<AtomicU64>) -> Self {
        Self {
            without_effects,
            quantizer: TpdfQuantizer::new(),
            quantized,
        }
    }

    integer_output!(i8, i8, 8, 0);
    integer_output!(i16, i16, 16, 0);
    integer_output!(i24, i32, 24, 0);
    integer_output!(i32, i32, 32, 0);
    integer_output!(u8, u8, 8, 1_i64 << 7);
    integer_output!(u16, u16, 16, 1_i64 << 15);
    integer_output!(u24, u32, 24, 1_i64 << 23);
    integer_output!(u32, u32, 32, 1_i64 << 31);
}

fn exact_signed(sample: f32, bits: u32) -> Option<i64> {
    let half_range = 1_i64 << (bits - 1);
    let scaled = f64::from(sample) * half_range as f64;
    // A wider float here prevents endpoint rounding from manufacturing an
    // integer outside the output range. Powers of two retain the input value.
    (scaled.is_finite()
        && scaled >= -half_range as f64
        && scaled <= (half_range - 1) as f64
        && scaled.fract() == 0.0)
        .then_some(scaled as i64)
}

#[cfg(test)]
#[path = "play_sample_conversion_tests.rs"]
mod tests;
