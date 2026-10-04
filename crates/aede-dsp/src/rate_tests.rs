use super::*;

#[test]
fn converted_stream_has_exact_frame_count_and_no_silent_boundary() {
    let input = PcmFormat::new(44_100, 2).expect("stereo");
    let mut converter = RateConverter::new(input, 48_000).expect("converter");
    let pcm = vec![0.25; 44_100 * 2];
    let mut output = Vec::new();
    for chunk in pcm.chunks(733 * 2) {
        output.extend_from_slice(converter.push(chunk, false).expect("chunk"));
    }
    output.extend_from_slice(converter.push(&[], true).expect("tail"));
    assert_eq!(output.len(), 48_000 * 2);
    assert!(
        output[8_000..80_000]
            .iter()
            .all(|sample| (*sample - 0.25).abs() < 0.001)
    );
}

#[test]
fn conversion_does_not_depend_on_source_block_boundaries() {
    let format = PcmFormat::new(48_000, 1).expect("mono");
    let pcm = (0..10_007)
        .map(|index| (index as f32 * 0.003).sin() * 0.6)
        .collect::<Vec<_>>();
    let mut one = RateConverter::new(format, 44_100).expect("converter");
    let mut split = RateConverter::new(format, 44_100).expect("converter");
    let all = one.push(&pcm, true).expect("all").to_vec();
    let mut blocks = Vec::new();
    for chunk in pcm.chunks(197) {
        blocks.extend_from_slice(split.push(chunk, false).expect("chunk"));
    }
    blocks.extend_from_slice(split.push(&[], true).expect("tail"));
    assert_eq!(all, blocks);
    assert_eq!(all.len(), 9_194);
}

#[test]
fn invalid_pcm_does_not_consume_converter_state() {
    let format = PcmFormat::new(44_100, 2).expect("stereo");
    let mut converter = RateConverter::new(format, 48_000).expect("converter");
    assert!(matches!(
        converter.push(&[0.1], false),
        Err(RateError::IncompleteFrame)
    ));
    assert!(matches!(
        converter.push(&[f32::NAN, 0.0], false),
        Err(RateError::NonFiniteSample)
    ));
    assert!(converter.push(&[0.1, 0.2], true).is_ok());
    assert!(matches!(
        converter.push(&[], true),
        Err(RateError::Finished)
    ));
}

#[test]
fn finite_extremes_are_refused_before_conversion_and_normal_pcm_still_works() {
    for input_rate in [44_100, 44_101] {
        let format = PcmFormat::new(input_rate, 1).expect("mono");
        let mut converter = RateConverter::new(format, 48_000).expect("converter");
        let samples = [1.25; 4_096];
        let mut fresh = RateConverter::new(format, 48_000).expect("fresh converter");
        converter.push(&samples[..777], false).expect("prefix");
        fresh
            .push(&samples[..777], false)
            .expect("reference prefix");
        assert!(matches!(
            converter.push(&vec![f32::MAX; 4_096], true),
            Err(RateError::SampleOverflow)
        ));
        let converted = converter
            .push(&samples[777..], true)
            .expect("normal signal");
        assert_eq!(
            converted,
            fresh.push(&samples[777..], true).expect("reference signal")
        );
        assert!(converted.iter().any(|sample| *sample > 1.0));
    }
}

#[test]
fn downsampling_preserves_passband_and_rejects_aliasing() {
    fn converted_sine(frequency: f32) -> Vec<f32> {
        let format = PcmFormat::new(48_000, 1).expect("mono");
        let mut converter = RateConverter::new(format, 16_000).expect("converter");
        let input = (0..48_000)
            .map(|frame| {
                (2.0 * std::f32::consts::PI * frequency * frame as f32 / 48_000.0).sin() * 0.5
            })
            .collect::<Vec<_>>();
        converter.push(&input, true).expect("conversion").to_vec()
    }
    fn rms(samples: &[f32]) -> f64 {
        let energy = samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum::<f64>();
        (energy / samples.len() as f64).sqrt()
    }
    let passband = converted_sine(1_000.0);
    let rejected = converted_sine(12_000.0);
    assert!(rms(&passband[2_000..14_000]) > 0.34);
    assert!(rms(&rejected[2_000..14_000]) < 0.005);
}

#[test]
fn uncommon_rate_pair_converts_without_an_oversized_fft() {
    let format = PcmFormat::new(44_101, 1).expect("mono");
    let mut converter = RateConverter::new(format, 48_000).expect("converter");
    let mut output = Vec::new();
    for chunk in vec![0.2; 44_101].chunks(511) {
        output.extend_from_slice(converter.push(chunk, false).expect("chunk"));
    }
    output.extend_from_slice(converter.push(&[], true).expect("tail"));
    assert_eq!(output.len(), 48_000);
    assert!(output.iter().all(|sample| sample.is_finite()));
}
