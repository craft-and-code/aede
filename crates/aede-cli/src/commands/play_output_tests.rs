#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn native_callback_preserves_samples_across_buffer_boundaries() {
    let (mut producer, mut pending) = super::queue::pcm_queue::<f32>(1, 5, false).unwrap();
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
    let (mut producer, mut pending) = super::queue::pcm_queue::<f32>(1, 2, false).unwrap();
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
    let (mut producer, _consumer) = super::queue::pcm_queue::<f32>(1, 4, false).unwrap();
    let sample = |value: f32| value.to_le_bytes();
    assert!(producer.write_f32le(&sample(0.5)).is_ok());
    assert!(producer.write_f32le(&sample(f32::NAN)).is_err());
    assert!(producer.write_f32le(&sample(1.1)).is_err());
    assert!(producer.write_f32le(&[0, 1, 2]).is_err());
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn a_slow_next_file_counts_only_the_silence_beyond_buffered_audio_and_recovers_in_order() {
    // Virtual 10 ms callbacks at 48 kHz consume a full 500 ms queue while the
    // producer prepares the next file. No wall-clock or device timing is used.
    let capacity_frames = 24_000;
    let callback_frames = 480;
    let next_frames = 4_096;
    for delay_ms in [200, 600] {
        let (mut producer, mut pending) =
            super::queue::pcm_queue::<f32>(2, capacity_frames, false).unwrap();
        let first = [0.125, -0.25].repeat(capacity_frames);
        let next = [0.375, -0.5].repeat(next_frames);
        assert_eq!(producer.write_samples(&first).unwrap(), first.len());
        let mut rendered = [0.0; 480 * 2];
        let mut heard = Vec::new();
        for _ in 0..delay_ms / 10 {
            pending.render_mapped(&mut rendered, 0.0, |sample| sample);
            for frame in rendered.as_chunks::<2>().0 {
                if frame[0] == 0.0 {
                    assert_eq!(*frame, [0.0, 0.0]);
                } else {
                    heard.extend_from_slice(frame);
                }
            }
        }
        let missing_callbacks = (delay_ms / 10_u64).saturating_sub(50);
        let before_refill = producer.snapshot();
        assert_eq!(before_refill.underrun_callbacks, missing_callbacks);
        assert_eq!(
            before_refill.underrun_frames,
            missing_callbacks * callback_frames
        );
        assert_eq!(producer.write_samples(&next).unwrap(), next.len());
        producer.close_input();
        while !producer.drained() {
            pending.render_mapped(&mut rendered, 0.0, |sample| sample);
            for frame in rendered.as_chunks::<2>().0 {
                if frame[0] == 0.0 {
                    assert_eq!(*frame, [0.0, 0.0]);
                } else {
                    heard.extend_from_slice(frame);
                }
            }
        }
        let mut expected = first;
        expected.extend_from_slice(&next);
        assert_eq!(heard, expected);
        let finished = producer.snapshot();
        assert_eq!(finished.queued_frames, 0);
        assert_eq!(
            finished.consumed_frames,
            (capacity_frames + next_frames) as u64
        );
        assert_eq!(finished.underrun_frames, before_refill.underrun_frames);
        assert_eq!(
            finished.underrun_callbacks,
            before_refill.underrun_callbacks
        );
    }
}
