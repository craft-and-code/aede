#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_callback_preserves_samples_across_buffer_boundaries() {
    let (mut producer, mut pending) = super::queue::pcm_queue(1, 5).unwrap();
    assert_eq!(producer.write_samples(&[0.1, 0.2, 0.3]).unwrap(), 3);
    assert_eq!(producer.write_samples(&[0.4, 0.5]).unwrap(), 2);
    let mut first = [0.0; 2];
    let mut second = [0.0; 3];
    pending.render_mapped(&mut first, 0.0, |sample| sample);
    pending.render_mapped(&mut second, 0.0, |sample| sample);
    assert_eq!(first, [0.1, 0.2]);
    assert_eq!(second, [0.3, 0.4, 0.5]);
    assert_eq!(producer.snapshot().consumed_frames, 5);
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_output_keeps_source_rate_or_chooses_nearest_compatible_rate() {
    use cpal::SampleFormat;

    let ranges = [
        (SampleFormat::F32, 48_000, 48_000),
        (SampleFormat::F32, 44_100, 96_000),
    ];
    assert_eq!(
        super::native::select_config(44_100, &ranges),
        Some((1, 44_100))
    );
    assert_eq!(
        super::native::select_config(32_000, &ranges),
        Some((1, 44_100))
    );
    assert_eq!(
        super::native::select_config(192_000, &ranges),
        Some((1, 96_000))
    );
    assert_eq!(
        super::native::select_config(48_000, &[(SampleFormat::F32, 48_000, 48_000)]),
        Some((0, 48_000))
    );
    assert_eq!(super::native::select_config(48_000, &[]), None);
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_output_prefers_float_and_uses_integer_when_needed() {
    use cpal::SampleFormat;

    let ranges = [
        (SampleFormat::I16, 44_100, 44_100),
        (SampleFormat::F32, 48_000, 48_000),
    ];
    assert_eq!(
        super::native::select_config(44_100, &ranges),
        Some((1, 48_000))
    );
    assert_eq!(
        super::native::select_config(44_100, &ranges[..1]),
        Some((0, 44_100))
    );
    assert_eq!(
        super::native::select_config(44_100, &[(SampleFormat::I24, 48_000, 48_000)]),
        Some((0, 48_000))
    );
    assert_eq!(
        super::native::select_config(44_100, &[(SampleFormat::DsdU8, 44_100, 44_100)]),
        None
    );
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn integer_callback_dithers_audio_but_keeps_underrun_silence_exact() {
    let (mut producer, mut pending) = super::queue::pcm_queue(1, 2).unwrap();
    assert_eq!(producer.write_samples(&[0.5, 0.0]).unwrap(), 2);
    let mut quantizer = aede_dsp::TpdfQuantizer::new();
    let mut output = [0_i16; 4];
    pending.render_mapped(&mut output, 0, |sample| quantizer.i16(sample));
    assert!((16_383..=16_385).contains(&output[0]));
    assert_eq!(&output[2..], &[0, 0]);
    assert_eq!(producer.snapshot().consumed_frames, 2);
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_integer_path_rejects_non_finite_and_overfull_pcm() {
    let (mut producer, _consumer) = super::queue::pcm_queue(1, 4).unwrap();
    let sample = |value: f32| value.to_le_bytes();
    assert!(producer.write_f32le(&sample(0.5)).is_ok());
    assert!(producer.write_f32le(&sample(f32::NAN)).is_err());
    assert!(producer.write_f32le(&sample(1.1)).is_err());
    assert!(producer.write_f32le(&[0, 1, 2]).is_err());
}
