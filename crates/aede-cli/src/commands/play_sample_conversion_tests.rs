use super::*;

#[test]
fn without_effects_preserves_every_signed_16_bit_value_and_exact_widening() {
    let quantized = Arc::new(AtomicU64::new(0));
    let mut conversion = SampleConversion::new(true, Arc::clone(&quantized));
    for value in i32::from(i16::MIN)..=i32::from(i16::MAX) {
        let normalized = value as f32 / 32768.0;
        assert_eq!(i32::from(conversion.i16(normalized)), value);
        assert_eq!(conversion.i24(normalized), value << 8);
        assert_eq!(conversion.i32(normalized), value << 16);
        assert_eq!(i32::from(conversion.u16(normalized)) - 32768, value);
        assert_eq!(
            i64::from(conversion.u24(normalized)) - (1_i64 << 23),
            i64::from(value) << 8
        );
        assert_eq!(
            i64::from(conversion.u32(normalized)) - (1_i64 << 31),
            i64::from(value) << 16
        );
    }
    assert_eq!(quantized.load(Ordering::Relaxed), 0);
}

#[test]
fn representable_24_bit_low_bits_are_kept_without_dither() {
    let quantized = Arc::new(AtomicU64::new(0));
    let mut conversion = SampleConversion::new(true, Arc::clone(&quantized));
    for value in [
        -8_388_608, -8_388_607, -65_537, -257, -1, 0, 1, 257, 65_537, 8_388_607,
    ] {
        let normalized = value as f32 / 8388608.0;
        assert_eq!(conversion.i24(normalized), value);
        assert_eq!(conversion.i32(normalized), value << 8);
        assert_eq!(
            i64::from(conversion.u24(normalized)) - (1_i64 << 23),
            i64::from(value)
        );
    }
    assert_eq!(quantized.load(Ordering::Relaxed), 0);
}

#[test]
fn necessary_quantization_is_counted_and_retains_the_existing_dither_mapping() {
    let quantized = Arc::new(AtomicU64::new(0));
    let mut conversion = SampleConversion::new(true, Arc::clone(&quantized));
    let mut reference = TpdfQuantizer::new();
    assert_eq!(conversion.i16(0.125), 4096);
    assert_eq!(
        conversion.i16(1.0 / 8388608.0),
        reference.i16(1.0 / 8388608.0)
    );
    assert_eq!(conversion.i8(1.0 / 32768.0), reference.i8(1.0 / 32768.0));
    assert_eq!(conversion.u8(1.0), reference.u8(1.0));
    assert_eq!(quantized.load(Ordering::Relaxed), 3);
}

#[test]
fn dsp_conversion_retains_its_continuous_dither_sequence() {
    let quantized = Arc::new(AtomicU64::new(0));
    let mut conversion = SampleConversion::new(false, Arc::clone(&quantized));
    let mut reference = TpdfQuantizer::new();
    for sample in [0.0, 0.5, -0.5, 1.0, -1.0, 0.1, 0.0] {
        assert_eq!(conversion.i16(sample), reference.i16(sample));
    }
    assert_eq!(quantized.load(Ordering::Relaxed), 7);
}
