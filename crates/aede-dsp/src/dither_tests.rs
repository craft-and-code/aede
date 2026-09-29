use super::TpdfQuantizer;

#[test]
fn integer_endpoints_and_unsigned_silence_are_bounded() {
    let mut quantizer = TpdfQuantizer::new();
    assert_eq!(quantizer.i16(-1.0), i16::MIN);
    assert_eq!(quantizer.i16(1.0), i16::MAX);
    assert_eq!(quantizer.i24(-1.0), -8_388_608);
    assert_eq!(quantizer.i24(1.0), 8_388_607);
    assert_eq!(quantizer.i32(-1.0), i32::MIN);
    assert_eq!(quantizer.i32(1.0), i32::MAX);
    assert_eq!(quantizer.u16(-1.0), 0);
    assert_eq!(quantizer.u16(1.0), u16::MAX);
    assert_eq!(quantizer.u24(-1.0), 0);
    assert_eq!(quantizer.u24(1.0), 16_777_215);
    assert_eq!(quantizer.u32(-1.0), 0);
    assert_eq!(quantizer.u32(1.0), u32::MAX);
    let below_one = f32::from_bits(1.0_f32.to_bits() - 1);
    for _ in 0..100 {
        assert!(quantizer.i16(below_one) > 0);
        assert!(quantizer.u32(below_one) > 1_u32 << 31);
    }
}

#[test]
fn dither_is_zero_mean_and_decorrelates_zero_level_quantization() {
    let mut quantizer = TpdfQuantizer::new();
    let samples = (0..100_000).map(|_| quantizer.i16(0.0)).collect::<Vec<_>>();
    let mean = samples.iter().map(|value| f64::from(*value)).sum::<f64>() / samples.len() as f64;
    let nonzero = samples.iter().filter(|value| **value != 0).count();
    assert!(mean.abs() < 0.01, "mean {mean}");
    assert!(nonzero > 10_000 && nonzero < 25_000, "nonzero {nonzero}");
    assert!(samples.iter().all(|value| value.abs() <= 1));
}

#[test]
fn dither_sequence_is_continuous_across_blocks() {
    let pcm = (0..10_000)
        .map(|index| (index as f32 * 0.002).sin() * 0.2)
        .collect::<Vec<_>>();
    let mut one = TpdfQuantizer::new();
    let all = pcm
        .iter()
        .map(|sample| one.i16(*sample))
        .collect::<Vec<_>>();
    let mut split = TpdfQuantizer::new();
    let mut chunks = Vec::new();
    for chunk in pcm.chunks(137) {
        chunks.extend(chunk.iter().map(|sample| split.i16(*sample)));
    }
    assert_eq!(all, chunks);
}
