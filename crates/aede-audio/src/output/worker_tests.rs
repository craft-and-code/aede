use super::*;
use crate::{queue::pcm_queue, test_support::Wave};
#[test]
fn minimal_same_format_keeps_source_samples_without_normalization_or_eq() {
    let input = [0.001, -0.001, 0.5, -0.5];
    let wave = Wave::new(&input, 2);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let mut converter = Converter::new(&decoder, 48_000, 2).unwrap();
    assert_eq!(converter.adapt(&input, false).unwrap(), &input);
    assert!(converter.adapt(&[], true).unwrap().is_empty());
}
#[test]
fn mono_duplication_and_stereo_fold_down_keep_complete_frames() {
    let wave = Wave::new(&[0.25, -0.5], 1);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let mut converter = Converter::new(&decoder, 48_000, 2).unwrap();
    assert_eq!(
        converter.adapt(&[0.25, -0.5], false).unwrap(),
        &[0.25, 0.25, -0.5, -0.5]
    );
    let wave = Wave::new(&[0.5, -0.5, 0.25, 0.75], 2);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let mut converter = Converter::new(&decoder, 48_000, 1).unwrap();
    assert_eq!(
        converter.adapt(&[0.5, -0.5, 0.25, 0.75], false).unwrap(),
        &[0., 0.5]
    );
}
#[test]
fn conversion_uses_the_shared_streaming_resampler_and_flushes_its_tail() {
    let input = vec![0.; 4096];
    let wave = Wave::new(&input, 1);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let mut converter = Converter::new(&decoder, 44_100, 1).unwrap();
    let mut frames = 0;
    for chunk in input.chunks(257) {
        let out = converter.adapt(chunk, false).unwrap();
        assert!(out.iter().all(|v| v.is_finite()));
        frames += out.len();
    }
    frames += converter.adapt(&[], true).unwrap().len();
    assert_eq!(frames, (4096_usize * 44_100).div_ceil(48_000));
}
#[test]
fn a_full_queue_can_be_cancelled_without_consumption_or_further_publication() {
    let (mut producer, _consumer) = pcm_queue::<f32>(2, 1, false).unwrap();
    producer.write_samples(&[0.25, 0.25]).unwrap();
    let before = producer.snapshot();
    let signals = Signals::default();
    signals.cancel.store(true, Ordering::Release);
    publish(&mut producer, &[0.5, 0.5], &signals).unwrap();
    assert_eq!(producer.snapshot(), before);
}
#[test]
fn prefix_seek_discards_original_frames_and_only_publishes_the_remaining_audio() {
    let input = [0., 0.1, 0.2, 0.3, 0.4, 0.5];
    let wave = Wave::new(&input, 1);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let converter = Converter::new(&decoder, 48_000, 1).unwrap();
    let (producer, mut consumer) = pcm_queue::<f32>(1, 16, false).unwrap();
    produce(
        decoder,
        converter,
        producer,
        3. / 48_000.,
        &Signals::default(),
    )
    .unwrap();
    let mut out = [9.; 4];
    consumer.render_mapped(&mut out, 0., |v| v);
    assert_eq!(out, [0.3, 0.4, 0.5, 0.]);
    assert_eq!(consumer.consumed_frames(), 3);
    assert!(consumer.input_closed());
}
#[test]
fn decode_failure_never_closes_input_as_a_successful_eof() {
    let wave = Wave::new(&[f32::NAN, 0.], 2);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let converter = Converter::new(&decoder, 48_000, 2).unwrap();
    let (producer, consumer) = pcm_queue::<f32>(2, 16, false).unwrap();
    assert!(produce(decoder, converter, producer, 0., &Signals::default()).is_err());
    assert!(!consumer.input_closed());
    assert_eq!(consumer.available_frames(), 0);
}

#[test]
fn cancelling_an_in_flight_full_queue_retires_the_decoder_without_a_successful_eof() {
    use std::time::{Duration, Instant};
    let wave = Wave::new(&vec![0.25; 4096], 1);
    let decoder = PcmStreamDecoder::open(&wave.0).unwrap();
    let converter = Converter::new(&decoder, 48_000, 1).unwrap();
    let (producer, consumer) = pcm_queue::<f32>(1, 2, false).unwrap();
    let signals = Arc::new(Signals::default());
    let held = signals.clone();
    let (done, result) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = done.send(produce(decoder, converter, producer, 0., &held));
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while consumer.available_frames() < 2 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let full = consumer.available_frames() == 2;
    let running = result.try_recv().is_err();
    // Always retire the worker before asserting, including on a failed setup.
    signals.cancel.store(true, Ordering::Release);
    let retired = result.recv_timeout(Duration::from_secs(5));
    if retired.is_ok() {
        worker.join().unwrap();
    }
    assert!(full, "worker did not fill the bounded queue");
    assert!(
        running,
        "a full paused queue must retain its worker until cancellation"
    );
    assert!(retired.unwrap().is_ok());
    assert_eq!(consumer.available_frames(), 2);
    assert_eq!(consumer.consumed_frames(), 0);
    assert!(
        !consumer.input_closed(),
        "cancellation must not claim natural EOF"
    );
}
