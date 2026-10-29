use super::*;
use crate::queue::pcm_queue;
#[test]
fn pause_consumes_no_pcm_and_gain_does_not_mutate_queued_source() {
    let (mut tx, mut rx) = pcm_queue::<f32>(2, 4, false).unwrap();
    tx.write_samples(&[0.5, -0.5, 0.25, -0.25]).unwrap();
    tx.close_input();
    let signals = Signals::default();
    let gain = Gain::default();
    let mut out = [9_f32; 2];
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out, [0., 0.]);
    assert_eq!(tx.snapshot().consumed_frames, 0);
    gain.set_volume(0.5);
    signals.paused.store(false, Ordering::Release);
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out, [0.25, -0.25]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    assert!(!signals.finished.load(Ordering::Acquire));
    gain.set_muted(true);
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out, [0., 0.]);
    assert!(signals.finished.load(Ordering::Acquire));
    gain.set_muted(false);
    assert_eq!(gain.value(), 0.5);
}
#[test]
fn device_failure_cannot_look_like_completion_or_consume_source() {
    let (mut tx, mut rx) = pcm_queue::<f32>(2, 4, false).unwrap();
    tx.write_samples(&[0.5, 0.5]).unwrap();
    let signals = Signals::default();
    signals.paused.store(false, Ordering::Release);
    signals.failed.store(true, Ordering::Release);
    let mut out = [9_f32; 2];
    render(&mut out, &mut rx, &signals, &Gain::default());
    assert_eq!(out, [0., 0.]);
    assert_eq!(tx.snapshot().consumed_frames, 0);
    assert!(!signals.finished.load(Ordering::Acquire));
}
#[test]
fn starvation_does_not_advance_source_clock_and_invalid_volume_is_ignored() {
    let (_tx, mut rx) = pcm_queue::<f32>(2, 4, false).unwrap();
    let signals = Signals::default();
    signals.paused.store(false, Ordering::Release);
    let gain = Gain::default();
    gain.set_volume(f32::NAN);
    assert_eq!(gain.value(), 1.);
    let mut out = [9_f32; 6];
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out, [0.; 6]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    assert!(!signals.finished.load(Ordering::Acquire));
}

#[test]
fn i24_full_scale_endpoints_saturate_in_mono_and_stereo() {
    let below_full_scale = f32::from_bits(1_f32.to_bits() - 1);
    for channels in [1, 2] {
        let input: Vec<_> = [-1., 0., below_full_scale, 1.]
            .into_iter()
            .flat_map(|sample| std::iter::repeat_n(sample, channels))
            .collect();
        let (mut tx, mut rx) = pcm_queue::<f32>(channels, 4, false).unwrap();
        assert_eq!(tx.write_samples(&input).unwrap(), input.len());
        tx.close_input();
        let signals = Signals::default();
        signals.paused.store(false, Ordering::Release);
        let mut out = vec![cpal::I24::new(42).unwrap(); 6 * channels];
        render(&mut out, &mut rx, &signals, &Gain::default());
        let raw: Vec<_> = out.iter().map(|sample| sample.inner()).collect();
        assert!(
            raw.iter()
                .all(|sample| (-8_388_608..=8_388_607).contains(sample)),
            "{channels}-channel I24 output exceeds its raw range: {raw:?}"
        );
        let expected: Vec<_> = [-8_388_608, 0, 8_388_607, 8_388_607, 0, 0]
            .into_iter()
            .flat_map(|sample| std::iter::repeat_n(sample, channels))
            .collect();
        assert_eq!(raw, expected);
        let snapshot = tx.snapshot();
        assert_eq!(snapshot.consumed_frames, 4);
        assert_eq!(snapshot.queued_frames, 0);
        assert_eq!(snapshot.underrun_frames, 0);
        assert_eq!(snapshot.underrun_callbacks, 0);
        assert!(!snapshot.failed);
        assert_eq!(signals.consumed.load(Ordering::Acquire), 4);
        assert!(signals.finished.load(Ordering::Acquire));
        assert!(!signals.failed.load(Ordering::Acquire));
        assert_eq!(f32::from_bits(signals.level.load(Ordering::Relaxed)), 1.);
    }
}

#[test]
fn i24_gain_mute_pause_and_eof_keep_the_source_clock_and_silence() {
    let (mut tx, mut rx) = pcm_queue::<f32>(2, 3, false).unwrap();
    tx.write_samples(&[1., -1., 1., -1., 1., -1.]).unwrap();
    tx.close_input();
    let signals = Signals::default();
    let gain = Gain::default();
    let mut out = [cpal::I24::new(42).unwrap(); 2];
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out.map(|sample| sample.inner()), [0, 0]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    assert!(!signals.finished.load(Ordering::Acquire));
    gain.set_volume(0.5);
    signals.paused.store(false, Ordering::Release);
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out.map(|sample| sample.inner()), [4_194_304, -4_194_304]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    assert!(!signals.finished.load(Ordering::Acquire));
    gain.set_volume(1.);
    gain.set_muted(true);
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out.map(|sample| sample.inner()), [0, 0]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 2);
    assert_eq!(f32::from_bits(signals.level.load(Ordering::Relaxed)), 0.);
    assert!(!signals.finished.load(Ordering::Acquire));
    gain.set_muted(false);
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out.map(|sample| sample.inner()), [8_388_607, -8_388_608]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 3);
    assert!(signals.finished.load(Ordering::Acquire));
    render(&mut out, &mut rx, &signals, &gain);
    assert_eq!(out.map(|sample| sample.inner()), [0, 0]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 3);
    assert_eq!(f32::from_bits(signals.level.load(Ordering::Relaxed)), 0.);
    assert!(signals.finished.load(Ordering::Acquire));
    assert_eq!(tx.snapshot().underrun_frames, 0);
}

#[test]
fn final_worker_guard_full_scale_reaches_valid_i24_output() {
    use crate::{output::worker, test_support::Wave};
    for channels in [1, 2] {
        let input: Vec<_> = [1.5, -1.5]
            .into_iter()
            .flat_map(|sample| std::iter::repeat_n(sample, usize::from(channels)))
            .collect();
        let wave = Wave::new(&input, channels);
        let original = std::fs::read(&wave.0).unwrap();
        let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
        let converter = worker::Converter::new(&decoder, 48_000, channels).unwrap();
        let (producer, mut consumer) = pcm_queue::<f32>(usize::from(channels), 2, false).unwrap();
        let signals = Signals::default();
        worker::produce(decoder, converter, producer, 0., &signals).unwrap();
        assert!(consumer.input_closed());
        assert_eq!(consumer.available_frames(), 2);
        signals.paused.store(false, Ordering::Release);
        let mut out = vec![cpal::I24::new(42).unwrap(); 3 * usize::from(channels)];
        render(&mut out, &mut consumer, &signals, &Gain::default());
        let raw: Vec<_> = out.iter().map(|sample| sample.inner()).collect();
        let expected: Vec<_> = [8_388_607, -8_388_608, 0]
            .into_iter()
            .flat_map(|sample| std::iter::repeat_n(sample, usize::from(channels)))
            .collect();
        assert_eq!(raw, expected);
        assert_eq!(signals.consumed.load(Ordering::Acquire), 2);
        assert!(signals.finished.load(Ordering::Acquire));
        assert_eq!(std::fs::read(&wave.0).unwrap(), original);
    }
}

#[test]
fn floating_output_preserves_positive_full_scale_and_adjacent_samples() {
    let input = [-1., 0., f32::from_bits(1_f32.to_bits() - 1), 1.];
    macro_rules! check {
        ($sample:ty, $expected:expr) => {{
            let (mut tx, mut rx) = pcm_queue::<f32>(1, input.len(), false).unwrap();
            tx.write_samples(&input).unwrap();
            tx.close_input();
            let signals = Signals::default();
            signals.paused.store(false, Ordering::Release);
            let mut out: [$sample; 4] = [9.; 4];
            render(&mut out, &mut rx, &signals, &Gain::default());
            assert_eq!(out, $expected);
            assert_eq!(signals.consumed.load(Ordering::Acquire), 4);
            assert!(signals.finished.load(Ordering::Acquire));
        }};
    }
    check!(f32, input);
    check!(f64, input.map(f64::from));
}

#[test]
fn native_integer_output_formats_keep_full_scale_and_digital_silence() {
    macro_rules! check {
        ($sample:ty, $silence:expr) => {{
            let (mut tx, mut rx) = pcm_queue::<f32>(1, 3, false).unwrap();
            tx.write_samples(&[-1., 0., 1.]).unwrap();
            tx.close_input();
            let signals = Signals::default();
            signals.paused.store(false, Ordering::Release);
            let mut out: [$sample; 4] = [9; 4];
            render(&mut out, &mut rx, &signals, &Gain::default());
            assert_eq!(out, [<$sample>::MIN, $silence, <$sample>::MAX, $silence]);
            assert_eq!(signals.consumed.load(Ordering::Acquire), 3);
            assert!(signals.finished.load(Ordering::Acquire));
        }};
    }
    check!(i8, 0);
    check!(i16, 0);
    check!(i32, 0);
    check!(i64, 0);
    check!(u8, 128);
    check!(u16, 32_768);
    check!(u32, 2_147_483_648);
    check!(u64, 9_223_372_036_854_775_808);
}

#[test]
fn buffer_xrun_does_not_stop_queued_audio_or_reset_its_clock() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 2, false).unwrap();
    producer.write_samples(&[0.25, -0.25, 0.5, -0.5]).unwrap();
    producer.close_input();
    let signals = Signals::default();
    signals.paused.store(false, Ordering::Release);
    handle_output_error(cpal::ErrorKind::Xrun, &signals);
    assert!(!signals.failed.load(Ordering::Acquire));
    let mut output = [9_f32; 4];
    render(&mut output, &mut consumer, &signals, &Gain::default());
    assert_eq!(output, [0.25, -0.25, 0.5, -0.5]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 2);
    assert!(signals.finished.load(Ordering::Acquire));
}

#[test]
fn unrecoverable_device_notifications_retire_preview_without_consuming_source() {
    for kind in [
        cpal::ErrorKind::DeviceNotAvailable,
        cpal::ErrorKind::HostUnavailable,
        cpal::ErrorKind::PermissionDenied,
        cpal::ErrorKind::StreamInvalidated,
        cpal::ErrorKind::Other,
    ] {
        let (mut producer, mut consumer) = pcm_queue::<f32>(2, 1, false).unwrap();
        producer.write_samples(&[0.5, -0.5]).unwrap();
        producer.close_input();
        let signals = Signals::default();
        signals.paused.store(false, Ordering::Release);
        handle_output_error(kind, &signals);
        handle_output_error(cpal::ErrorKind::Xrun, &signals);
        let mut output = [9_f32; 2];
        render(&mut output, &mut consumer, &signals, &Gain::default());
        assert_eq!(output, [0., 0.], "{kind:?}");
        assert_eq!(signals.consumed.load(Ordering::Acquire), 0, "{kind:?}");
        assert!(!signals.finished.load(Ordering::Acquire), "{kind:?}");
        assert!(signals.failed.load(Ordering::Acquire), "{kind:?}");
    }
}
