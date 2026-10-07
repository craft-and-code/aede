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
fn without_effects_prefers_the_source_rate_before_sample_format() {
    use cpal::SampleFormat;
    let ranges = [
        (SampleFormat::F32, 48_000, 48_000),
        (SampleFormat::I24, 44_100, 44_100),
        (SampleFormat::I16, 44_100, 44_100),
    ];
    assert_eq!(
        super::native::select_config_without_effects(44_100, &ranges),
        Some((1, 44_100))
    );
    assert_eq!(
        super::native::select_config(44_100, &ranges),
        Some((0, 48_000))
    );
    let ranges = [
        (SampleFormat::F32, 96_000, 96_000),
        (SampleFormat::I32, 48_000, 48_000),
        (SampleFormat::F32, 44_100, 44_100),
    ];
    assert_eq!(
        super::native::select_config_without_effects(44_100, &ranges),
        Some((2, 44_100))
    );
    assert_eq!(
        super::native::select_config_without_effects(48_000, &ranges),
        Some((1, 48_000))
    );
    assert_eq!(
        super::native::select_config_without_effects(
            48_000,
            &[(SampleFormat::F32, 96_000, 48_000)]
        ),
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
#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn exact_native_callbacks_keep_integer_values_across_queue_boundaries() {
    use aede_core::playback::format::IntegerPcmFormat;
    use aede_dsp::ChannelLayout;
    for bits in [16, 24] {
        let format = IntegerPcmFormat::new(44_100, ChannelLayout::STEREO, bits).unwrap();
        let edge = 1 << (bits - 1);
        let samples = [-edge, edge - 1, -1, 1, 0, 127, -128, 129];
        let (mut producer, mut pending) = super::queue::pcm_queue::<i32>(2, 4, true).unwrap();
        assert_eq!(producer.write_integer(&samples[..4], format).unwrap(), 4);
        assert_eq!(producer.write_integer(&samples[4..], format).unwrap(), 4);
        producer.close_input();
        let mapping = super::native::ExactMapping(bits);
        let mut first = [0_i32; 2];
        let mut second = [0_i32; 6];
        pending.render_mapped(&mut first, 0, |s| mapping.i32(s));
        pending.render_mapped(&mut second, 0, |s| mapping.i32(s));
        let values = first.into_iter().chain(second).collect::<Vec<_>>();
        assert_eq!(values, samples.map(|s| s << (32 - bits)));
        assert_eq!(producer.snapshot().consumed_frames, 4);
        assert!(!producer.snapshot().failed);
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn exact_callback_representations_preserve_low_bits_and_signed_endpoints() {
    for bits in [16, 24] {
        let edge = 1_i32 << (bits - 1);
        let mapping = super::native::ExactMapping(bits);
        for sample in [
            -edge,
            -edge + 1,
            -129,
            -128,
            -1,
            0,
            1,
            127,
            128,
            edge - 2,
            edge - 1,
        ] {
            assert_eq!(mapping.i24(sample).inner(), sample << (24 - bits));
            assert_eq!(mapping.i32(sample), sample << (32 - bits));
            assert_eq!(
                f64::from(mapping.f32(sample)),
                f64::from(sample) / f64::from(edge)
            );
            assert_eq!(mapping.f64(sample), f64::from(sample) / f64::from(edge));
            if bits == 16 {
                assert_eq!(i32::from(mapping.i16(sample)), sample);
            }
        }
    }
    let mapping = super::native::ExactMapping(16);
    for sample in i32::from(i16::MIN)..=i32::from(i16::MAX) {
        assert_eq!(i32::from(mapping.i16(sample)), sample);
        assert_eq!(mapping.i24(sample).inner() >> 8, sample);
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn strict_output_remembers_pause_before_device_start() {
    use std::cell::Cell;
    let state = super::native::StartState::default();
    let plays = Cell::new(0);
    state
        .pause(|| panic!("an unopened stream must not be paused"))
        .unwrap();
    state
        .ensure(true, || {
            plays.set(plays.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(plays.get(), 0);
    state
        .resume(|| panic!("resume must not start an unprepared stream"))
        .unwrap();
    state
        .ensure(true, || {
            plays.set(plays.get() + 1);
            Ok(())
        })
        .unwrap();
    state
        .ensure(true, || {
            panic!("an active stream must not be started twice")
        })
        .unwrap();
    assert_eq!(plays.get(), 1);
}

#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    all(target_os = "linux", target_env = "gnu")
))]
#[test]
fn strict_prefill_preserves_accepted_progress_before_a_failed_device_start() {
    let format = aede_core::playback::format::IntegerPcmFormat::new(
        44_100,
        aede_dsp::ChannelLayout::MONO,
        24,
    )
    .unwrap();
    let (mut producer, mut pending) = super::queue::pcm_queue::<i32>(1, 2, true).unwrap();
    let state = super::native::StartState::default();
    state
        .ensure(false, || panic!("partial prefill must not start output"))
        .unwrap();
    let accepted = producer.write_integer(&[1, 2], format).unwrap();
    assert_eq!(accepted, 2);
    let result = state.ensure(true, || Err(std::io::Error::other("device start failed")));
    assert!(result.is_err());
    producer.failure_handle().fail();
    let mut output = [99; 2];
    pending.render_mapped(&mut output, 0, |s| s);
    assert_eq!(output, [0; 2]);
    assert_eq!(producer.snapshot().queued_frames, 2);
    assert_eq!(producer.snapshot().consumed_frames, 0);
    assert!(producer.write_integer(&[3], format).is_err());
}

#[test]
fn strict_selection_refuses_ffplay_and_survives_abort_without_fallback() {
    use super::{BackendChoice, LocalOutput, NativeSelection};
    let strict = NativeSelection {
        device: None,
        strict: true,
        ..Default::default()
    };
    assert!(LocalOutput::with_selection(BackendChoice::Ffplay, strict.clone()).is_err());
    let mut output = LocalOutput::with_selection(BackendChoice::Auto, strict).unwrap();
    assert!(output.requires_exact());
    output.abort().unwrap();
    assert!(output.requires_exact());
    let source = aede_core::playback::format::IntegerPcmFormat::new(
        44_100,
        aede_dsp::ChannelLayout::MONO,
        16,
    )
    .unwrap();
    assert!(output.prepare_exact(source).is_err());
    assert_eq!(output.stage_description(), "unopened");
    assert!(output.write_exact(&[1], &[1, 0, 0, 0], 0).is_err());
    assert!(output.write_exact(&[1], &[1, 0, 0, 0], 1).is_err());
    assert!(output.write_exact(&[1], &[], 0).is_err());
}
