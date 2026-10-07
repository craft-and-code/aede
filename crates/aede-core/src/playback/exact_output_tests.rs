use aede_dsp::ChannelLayout;

use super::{
    ExactOutputError, ExactOutputFormat, ExactPcmAdapter, ExactSampleRepresentation,
    MAX_EXACT_BLOCK_FRAMES,
};
use crate::playback::format::IntegerPcmFormat;

fn adapter(
    source_bits: u32,
    layout: ChannelLayout,
    representation: ExactSampleRepresentation,
    effective_bits: u32,
) -> ExactPcmAdapter {
    ExactPcmAdapter::new(
        IntegerPcmFormat::new(48_000, layout, source_bits).unwrap(),
        ExactOutputFormat::new(48_000, layout, representation, Some(effective_bits)).unwrap(),
    )
    .unwrap()
}

#[test]
fn exact_output_requires_explicit_rate_layout_and_valid_precision_facts() {
    use ExactSampleRepresentation::*;
    for (rate, layout, representation, bits) in [
        (0, ChannelLayout::MONO, Float32Le, Some(24)),
        (48_000, ChannelLayout::Unknown(2), Float32Le, Some(24)),
        (48_000, ChannelLayout::Mask(0), Float32Le, Some(24)),
    ] {
        assert!(matches!(
            ExactOutputFormat::new(rate, layout, representation, bits),
            Err(ExactOutputError::InvalidOutputFormat)
        ));
    }
    for (representation, bits) in [
        (Float32Le, 0),
        (Float64Le, 65),
        (Signed16Le, 17),
        (PackedSigned24Le, 25),
        (Signed32Le, 33),
    ] {
        assert!(matches!(
            ExactOutputFormat::new(48_000, ChannelLayout::MONO, representation, Some(bits)),
            Err(ExactOutputError::InvalidEffectivePrecision(actual)) if actual == bits
        ));
    }
    let source = IntegerPcmFormat::new(48_000, ChannelLayout::MONO, 24).unwrap();
    let unknown = ExactOutputFormat::new(48_000, ChannelLayout::MONO, Float32Le, None).unwrap();
    assert_eq!(unknown.effective_bits(), None);
    assert!(matches!(
        ExactPcmAdapter::new(source, unknown),
        Err(ExactOutputError::UnknownEffectivePrecision)
    ));
}

#[test]
fn exact_adapter_refuses_rate_layout_and_effective_precision_conversion() {
    use ExactSampleRepresentation::*;
    let source = IntegerPcmFormat::new(96_000, ChannelLayout::STEREO, 24).unwrap();
    let rate = ExactOutputFormat::new(48_000, ChannelLayout::STEREO, Signed32Le, Some(24)).unwrap();
    assert!(matches!(
        ExactPcmAdapter::new(source, rate),
        Err(ExactOutputError::RateMismatch {
            source: 96_000,
            output: 48_000
        })
    ));
    for layout in [ChannelLayout::MONO, ChannelLayout::Mask(0xc)] {
        let output = ExactOutputFormat::new(96_000, layout, Signed32Le, Some(24)).unwrap();
        assert!(matches!(
            ExactPcmAdapter::new(source, output),
            Err(ExactOutputError::LayoutMismatch {
                source: ChannelLayout::STEREO,
                output: actual,
            }) if actual == layout
        ));
    }
    for (representation, bits) in [
        (Signed16Le, 16),
        (PackedSigned24Le, 23),
        (Signed32Le, 16),
        (Float32Le, 23),
        (Float64Le, 16),
    ] {
        let output =
            ExactOutputFormat::new(96_000, ChannelLayout::STEREO, representation, Some(bits))
                .unwrap();
        assert!(matches!(
            ExactPcmAdapter::new(source, output),
            Err(ExactOutputError::InsufficientPrecision {
                source: 24,
                output: actual,
            }) if actual == bits
        ));
    }
    // A wide integer container does not manufacture effective DAC precision.
    let output =
        ExactOutputFormat::new(96_000, ChannelLayout::STEREO, Signed32Le, Some(24)).unwrap();
    let exact = ExactPcmAdapter::new(source, output).unwrap();
    assert_eq!(exact.source_format(), source);
    assert_eq!(exact.output_format(), output);
    assert_eq!(output.sample_rate(), 96_000);
    assert_eq!(output.layout(), ChannelLayout::STEREO);
    assert_eq!(output.channels(), 2);
    assert_eq!(output.representation(), Signed32Le);
    assert_eq!(output.effective_bits(), Some(24));
}

#[test]
fn invalid_source_samples_frames_and_oversized_blocks_leave_encoded_bytes_unchanged() {
    use ExactSampleRepresentation::*;
    for bits in [16, 24] {
        for representation in [PackedSigned24Le, Signed32Le, Float32Le, Float64Le] {
            let exact = adapter(bits, ChannelLayout::STEREO, representation, 24);
            let mut bytes = vec![0x35, 0xe8, 0x71];
            let unchanged = bytes.clone();
            assert!(matches!(
                exact.encode_block(&[0, 1, 2], &mut bytes),
                Err(ExactOutputError::IncompleteFrame)
            ));
            assert_eq!(bytes, unchanged);
            for sample in [-(1 << (bits - 1)) - 1, 1 << (bits - 1), i32::MIN, i32::MAX] {
                assert!(matches!(
                    exact.encode_block(&[0, 1, 2, sample], &mut bytes),
                    Err(ExactOutputError::SampleOutOfRange {
                        index: 3,
                        sample: actual,
                        bits: actual_bits,
                    }) if actual == sample && actual_bits == bits
                ));
                assert_eq!(bytes, unchanged);
            }
            let oversized = vec![0; (MAX_EXACT_BLOCK_FRAMES + 1) * 2];
            assert!(matches!(
                exact.encode_block(&oversized, &mut bytes),
                Err(ExactOutputError::BlockTooLarge)
            ));
            assert_eq!(bytes, unchanged);
            let full = vec![0; MAX_EXACT_BLOCK_FRAMES * 2];
            assert_eq!(
                exact.encode_block(&full, &mut bytes).unwrap(),
                MAX_EXACT_BLOCK_FRAMES
            );
            assert!(bytes.iter().all(|byte| *byte == 0));
            assert_eq!(bytes.len(), full.len() * representation.bytes_per_sample());
            assert_eq!(exact.encode_block(&[], &mut bytes).unwrap(), 0);
            assert!(bytes.is_empty());
        }
    }
    let exact = adapter(16, ChannelLayout::MONO, Signed16Le, 16);
    let mut bytes = vec![0xab];
    assert!(matches!(
        exact.encode_block(&[0, 32_768], &mut bytes),
        Err(ExactOutputError::SampleOutOfRange { index: 1, .. })
    ));
    assert_eq!(bytes, [0xab]);
}

#[test]
fn signed_output_endpoints_have_explicit_full_width_little_endian_packing() {
    use ExactSampleRepresentation::*;
    for (bits, representation, precision, expected) in [
        (
            16,
            Signed16Le,
            16,
            vec![0x00, 0x80, 0xff, 0xff, 0x00, 0x00, 0x01, 0x00, 0xff, 0x7f],
        ),
        (
            16,
            PackedSigned24Le,
            24,
            vec![
                0x00, 0x00, 0x80, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xff,
                0x7f,
            ],
        ),
        (
            16,
            Signed32Le,
            32,
            vec![
                0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x01, 0x00, 0x00, 0x00, 0xff, 0x7f,
            ],
        ),
        (
            24,
            PackedSigned24Le,
            24,
            vec![
                0x00, 0x00, 0x80, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xff, 0xff,
                0x7f,
            ],
        ),
        (
            24,
            Signed32Le,
            32,
            vec![
                0x00, 0x00, 0x00, 0x80, 0x00, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
                0x00, 0x00, 0x00, 0xff, 0xff, 0x7f,
            ],
        ),
    ] {
        let samples = [-(1 << (bits - 1)), -1, 0, 1, (1 << (bits - 1)) - 1];
        let exact = adapter(bits, ChannelLayout::MONO, representation, precision);
        let mut bytes = Vec::new();
        assert_eq!(exact.encode_block(&samples, &mut bytes).unwrap(), 5);
        assert_eq!(bytes, expected, "{bits}-bit {representation:?}");
    }
}

fn recover_signed(bytes: &[u8], representation: ExactSampleRepresentation) -> (i64, u32) {
    match representation {
        ExactSampleRepresentation::Signed16Le => {
            (i64::from(i16::from_le_bytes(bytes.try_into().unwrap())), 16)
        }
        ExactSampleRepresentation::PackedSigned24Le => {
            let sign = if bytes[2] & 0x80 == 0 { 0 } else { 0xff };
            (
                i64::from(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], sign])),
                24,
            )
        }
        ExactSampleRepresentation::Signed32Le => {
            (i64::from(i32::from_le_bytes(bytes.try_into().unwrap())), 32)
        }
        _ => panic!("signed recovery requires an integer representation"),
    }
}

#[test]
fn signed_widening_and_packing_preserve_every_sixteen_bit_source_value() {
    use ExactSampleRepresentation::*;
    for representation in [Signed16Le, PackedSigned24Le, Signed32Le] {
        let precision = match representation {
            Signed16Le => 16,
            PackedSigned24Le => 24,
            Signed32Le => 32,
            _ => unreachable!(),
        };
        let exact = adapter(16, ChannelLayout::MONO, representation, precision);
        let mut source = Vec::with_capacity(MAX_EXACT_BLOCK_FRAMES);
        let mut encoded = Vec::new();
        for first in (-32_768..32_768).step_by(MAX_EXACT_BLOCK_FRAMES) {
            source.clear();
            source.extend(first..first + MAX_EXACT_BLOCK_FRAMES as i32);
            assert_eq!(
                exact.encode_block(&source, &mut encoded).unwrap(),
                source.len()
            );
            for (&original, bytes) in source
                .iter()
                .zip(encoded.chunks_exact(representation.bytes_per_sample()))
            {
                let (output, width) = recover_signed(bytes, representation);
                assert_eq!(output, i64::from(original) * (1_i64 << (width - 16)));
                assert_eq!(output >> (width - 16), i64::from(original));
            }
        }
    }
}

#[test]
fn twenty_four_bit_integer_outputs_preserve_low_bits_and_distinguishable_stereo_order() {
    use ExactSampleRepresentation::*;
    let mut source = vec![-8_388_608, 8_388_607, -1, 0, 1, 255, 256, -257];
    for bit in 0..23 {
        source.extend([1 << bit, -(1 << bit), (1 << bit) - 1, -(1 << bit) - 1]);
    }
    for byte in 0..=255 {
        source.extend([byte, byte << 8, (byte << 24) >> 8, !byte]);
    }
    for representation in [PackedSigned24Le, Signed32Le] {
        let exact = adapter(24, ChannelLayout::STEREO, representation, 24);
        let mut bytes = Vec::new();
        assert_eq!(
            exact.encode_block(&source, &mut bytes).unwrap(),
            source.len() / 2
        );
        for (&original, encoded) in source
            .iter()
            .zip(bytes.chunks_exact(representation.bytes_per_sample()))
        {
            let (output, width) = recover_signed(encoded, representation);
            assert_eq!(output, i64::from(original) * (1_i64 << (width - 24)));
            assert_eq!(output >> (width - 24), i64::from(original));
        }
        let mut separated = Vec::new();
        for frame in source.as_chunks::<2>().0 {
            let mut encoded = Vec::new();
            exact.encode_block(frame, &mut encoded).unwrap();
            separated.extend(encoded);
        }
        assert_eq!(bytes, separated);
    }
}

#[test]
fn float_output_preserves_every_sixteen_and_twenty_four_bit_value_exactly() {
    use ExactSampleRepresentation::*;
    // Iterate the complete signed domains in bounded batches. binary64 is an
    // independent exact oracle for these integer/power-of-two rational values;
    // no decoding or processed float buffers supply the expected samples.
    for bits in [16, 24] {
        let lower = -(1_i32 << (bits - 1));
        let end = 1_i32 << (bits - 1);
        let denominator = f64::from(end);
        let mut source = Vec::with_capacity(MAX_EXACT_BLOCK_FRAMES);
        let f32_adapter = adapter(bits, ChannelLayout::MONO, Float32Le, 24);
        let f64_adapter = adapter(bits, ChannelLayout::MONO, Float64Le, 32);
        let mut encoded_f32 = Vec::new();
        let mut encoded_f64 = Vec::new();
        let mut seen = 0_u64;
        for first in (lower..end).step_by(MAX_EXACT_BLOCK_FRAMES) {
            source.clear();
            source.extend(first..(first + MAX_EXACT_BLOCK_FRAMES as i32).min(end));
            assert_eq!(
                f32_adapter.encode_block(&source, &mut encoded_f32).unwrap(),
                source.len()
            );
            assert_eq!(
                f64_adapter.encode_block(&source, &mut encoded_f64).unwrap(),
                source.len()
            );
            for ((&original, f32_bytes), f64_bytes) in source
                .iter()
                .zip(encoded_f32.as_chunks::<4>().0)
                .zip(encoded_f64.as_chunks::<8>().0)
            {
                let actual_f32 = f32::from_le_bytes(*f32_bytes);
                let actual_f64 = f64::from_le_bytes(*f64_bytes);
                let expected = f64::from(original) / denominator;
                assert_eq!(f64::from(actual_f32), expected, "{bits}-bit {original}");
                assert_eq!(actual_f64, expected, "{bits}-bit {original}");
                assert_eq!(f64::from(actual_f32) * denominator, f64::from(original));
                assert_eq!(actual_f64 * denominator, f64::from(original));
                if original == 0 {
                    assert_eq!(actual_f32.to_bits(), 0);
                    assert_eq!(actual_f64.to_bits(), 0);
                }
            }
            seen += source.len() as u64;
        }
        assert_eq!(seen, 1_u64 << bits);
    }
}

#[test]
fn output_buffer_capacity_is_reused_between_encodings_without_appending_old_audio() {
    let exact = adapter(
        24,
        ChannelLayout::STEREO,
        ExactSampleRepresentation::Signed32Le,
        24,
    );
    let mut bytes = Vec::with_capacity(MAX_EXACT_BLOCK_FRAMES * 8);
    let capacity = bytes.capacity();
    exact.encode_block(&[1, -1, 2, -2], &mut bytes).unwrap();
    assert_eq!(bytes.len(), 16);
    let first_frame = bytes[..8].to_vec();
    exact.encode_block(&[1, -1], &mut bytes).unwrap();
    assert_eq!(bytes, first_frame);
    assert_eq!(bytes.capacity(), capacity);
}
