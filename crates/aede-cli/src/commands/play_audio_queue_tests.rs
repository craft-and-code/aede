use std::io;
use std::thread;
use std::time::{Duration, Instant};

use super::pcm_queue;
use aede_core::playback::format::IntegerPcmFormat;
use aede_dsp::ChannelLayout;

#[test]
fn host_failure_stops_queued_source_without_consumption_or_conversion() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(2, 3, true).expect("queue");
    producer.write_samples(&[-1, 1, -2, 2]).expect("source");
    let mut output = [9; 2];
    consumer.render_mapped(&mut output, 0, |sample| sample);
    assert_eq!(output, [-1, 1]);
    producer.failure_handle().fail();
    let failed = producer.snapshot();
    assert!(failed.failed);
    assert_eq!(failed.queued_frames, 1);
    assert_eq!(failed.consumed_frames, 1);
    assert_eq!(
        producer
            .write_samples(&[3, -3])
            .expect_err("host failed")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    producer.close_input();
    let mut calls = 0;
    consumer.render_mapped(&mut output, 0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(output, [0; 2]);
    assert_eq!(calls, 0);
    assert_eq!(producer.snapshot(), failed);
}

#[test]
fn host_failure_before_startup_cannot_be_cleared_by_publication_or_closure() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(1, 1, true).expect("queue");
    let failure = producer.failure_handle();
    failure.clone().fail();
    assert_eq!(
        producer
            .write_samples(&[1])
            .expect_err("failed before startup")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    producer.close_input();
    let mut output = [9];
    consumer.render_mapped(&mut output, 0, |_| {
        panic!("failed callback must not convert")
    });
    assert_eq!(output, [0]);
    let state = producer.snapshot();
    assert!(state.failed);
    assert_eq!(state.consumed_frames, 0);
    assert_eq!(state.underrun_callbacks, 0);
}

#[test]
fn integer_source_depth_and_layout_are_checked_before_any_prefix_is_published() {
    for bits in [16, 24] {
        let source = IntegerPcmFormat::new(48_000, ChannelLayout::STEREO, bits).expect("source");
        let wrong_channels =
            IntegerPcmFormat::new(48_000, ChannelLayout::MONO, bits).expect("mono");
        let limit = 1_i32 << (bits - 1);
        let (mut producer, mut consumer) = pcm_queue::<i32>(2, 1, true).expect("queue");
        for invalid in [vec![0, 1, limit, 0], vec![0, 1, -limit - 1, 0], vec![0]] {
            assert_eq!(
                producer
                    .write_integer(&invalid, source)
                    .expect_err("invalid whole block")
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(producer.snapshot().queued_frames, 0);
        }
        assert_eq!(
            producer
                .write_integer(&[0, 1], wrong_channels)
                .expect_err("layout mismatch")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            producer
                .write_integer(&[-limit, limit - 1], source)
                .expect("endpoints"),
            2
        );
        assert_eq!(
            producer
                .write_integer(&[0, limit], source)
                .expect_err("invalid even when full")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        producer.close_input();
        let mut output = [9; 2];
        consumer.render_mapped(&mut output, 0, |sample| sample);
        assert_eq!(output, [-limit, limit - 1]);
        assert!(!producer.snapshot().failed);
    }
}

fn f32le(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

#[test]
fn frame_aligned_prefixes_preserve_channel_order_through_wrap_and_retry() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 3, false).expect("queue");
    let source = [0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.4, -0.4, 0.5, -0.5];
    assert_eq!(producer.write_samples(&source).expect("prefix"), 6);
    assert_eq!(producer.snapshot().queued_frames, 3);
    assert_eq!(producer.snapshot().capacity_frames, 3);
    assert_eq!(
        producer
            .write_samples(&source[6..])
            .expect_err("full")
            .kind(),
        io::ErrorKind::WouldBlock
    );
    let mut first = [0.0; 4];
    consumer.render_mapped(&mut first, 0.0, |sample| sample);
    assert_eq!(first, source[..4]);
    assert_eq!(producer.snapshot().consumed_frames, 2);
    assert_eq!(
        producer.write_samples(&source[6..]).expect("wrapped write"),
        4
    );
    let mut second = [0.0; 6];
    consumer.render_mapped(&mut second, 0.0, |sample| sample);
    assert_eq!(second, source[4..]);
    assert_eq!(producer.snapshot().consumed_frames, 5);
    assert!(producer.drained());
}

#[test]
fn f32le_prefixes_report_bytes_and_keep_exact_pcm() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 2, false).expect("queue");
    let source = [0.1, -0.2, 0.3, -0.4, 0.5, -0.6];
    let bytes = f32le(&source);
    assert_eq!(producer.write_f32le(&bytes).expect("prefix bytes"), 16);
    let mut first = [0.0; 2];
    consumer.render_mapped(&mut first, 0.0, |sample| sample);
    assert_eq!(first, source[..2]);
    assert_eq!(
        producer.write_f32le(&bytes[16..]).expect("wrapped bytes"),
        8
    );
    let mut remaining = [0.0; 4];
    consumer.render_mapped(&mut remaining, 0.0, |sample| sample);
    assert_eq!(remaining, source[2..]);
    assert_eq!(producer.snapshot().consumed_frames, 3);
}

#[test]
fn whole_input_is_validated_before_a_prefix_can_be_published() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 1, false).expect("queue");
    for bad in [
        vec![0.1],
        vec![0.1, -0.1, f32::NAN, 0.0],
        vec![0.1, -0.1, f32::INFINITY, 0.0],
        vec![0.1, -0.1, 1.001, 0.0],
        vec![0.1, -0.1, -1.001, 0.0],
    ] {
        assert_eq!(
            producer
                .write_samples(&bad)
                .expect_err("invalid PCM")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(producer.drained());
        assert_eq!(producer.snapshot().queued_frames, 0);
    }
    for bad in [
        vec![0; 3],
        f32le(&[0.1]),
        f32le(&[0.1, -0.1, 0.0, f32::NAN]),
    ] {
        assert_eq!(
            producer
                .write_f32le(&bad)
                .expect_err("invalid bytes")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(producer.drained());
    }
    producer.write_samples(&[0.2, -0.3]).expect("valid frame");
    assert_eq!(
        producer
            .write_samples(&[0.1, f32::NAN])
            .expect_err("invalid even while full")
            .kind(),
        io::ErrorKind::InvalidInput
    );
    let mut output = [0.0; 2];
    consumer.render_mapped(&mut output, 0.0, |sample| sample);
    assert_eq!(output, [0.2, -0.3]);
    assert_eq!(producer.snapshot().consumed_frames, 1);
}

#[test]
fn partial_callback_frames_are_silent_and_never_shift_channels() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 3, false).expect("queue");
    producer
        .write_samples(&[0.1, -0.1, 0.2, -0.2])
        .expect("two frames");
    let mut calls = 0;
    let mut shorter_than_frame = [9.0];
    consumer.render_mapped(&mut shorter_than_frame, 0.0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(shorter_than_frame, [0.0]);
    assert_eq!(calls, 0);
    assert_eq!(producer.snapshot().consumed_frames, 0);
    assert_eq!(producer.snapshot().underrun_callbacks, 0);
    let mut odd_length = [9.0; 3];
    consumer.render_mapped(&mut odd_length, 0.0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(odd_length, [0.1, -0.1, 0.0]);
    assert_eq!(calls, 2);
    assert_eq!(producer.snapshot().consumed_frames, 1);
    let mut last = [9.0; 2];
    consumer.render_mapped(&mut last, 0.0, |sample| sample);
    assert_eq!(last, [0.2, -0.2]);
    assert_eq!(producer.snapshot().consumed_frames, 2);
}

#[test]
fn startup_empty_output_and_closed_tail_silence_are_not_underruns() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 2, false).expect("queue");
    let mut calls = 0;
    let mut startup = [7u16; 8];
    consumer.render_mapped(&mut startup, 32_768, |_| {
        calls += 1;
        0
    });
    assert_eq!(startup, [32_768; 8]);
    assert_eq!(calls, 0);
    assert_eq!(producer.snapshot().underrun_frames, 0);
    assert_eq!(producer.snapshot().underrun_callbacks, 0);
    producer.write_samples(&[0.25, -0.25]).expect("one frame");
    consumer.render_mapped(&mut [], 32_768, |_| {
        calls += 1;
        0
    });
    assert_eq!(producer.snapshot().queued_frames, 1);
    assert_eq!(producer.snapshot().underrun_callbacks, 0);
    producer.close_input();
    let mut tail = [7u16; 6];
    consumer.render_mapped(&mut tail, 32_768, |sample| {
        calls += 1;
        if sample > 0.0 { 40_000 } else { 20_000 }
    });
    assert_eq!(tail, [40_000, 20_000, 32_768, 32_768, 32_768, 32_768]);
    assert_eq!(calls, 2);
    assert!(producer.drained());
    assert_eq!(producer.snapshot().consumed_frames, 1);
    assert_eq!(producer.snapshot().underrun_frames, 0);
    assert_eq!(producer.snapshot().underrun_callbacks, 0);
    assert_eq!(
        producer
            .write_samples(&[0.0, 0.0])
            .expect_err("closed")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[test]
fn underruns_count_missing_complete_frames_and_resume_after_refill() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 3, false).expect("queue");
    producer
        .write_samples(&[0.1, -0.1, 0.2, -0.2, 0.3, -0.3])
        .expect("three frames");
    let mut output = [9.0; 8];
    consumer.render_mapped(&mut output, 0.0, |sample| sample);
    assert_eq!(output, [0.1, -0.1, 0.2, -0.2, 0.3, -0.3, 0.0, 0.0]);
    let first = producer.snapshot();
    assert_eq!(first.consumed_frames, 3);
    assert_eq!(first.underrun_frames, 1);
    assert_eq!(first.underrun_callbacks, 1);
    consumer.render_mapped(&mut output[..4], 0.0, |sample| sample);
    let second = producer.snapshot();
    assert_eq!(second.consumed_frames, 3);
    assert_eq!(second.underrun_frames, 3);
    assert_eq!(second.underrun_callbacks, 2);
    producer.write_samples(&[0.4, -0.4]).expect("refill");
    consumer.render_mapped(&mut output[..2], 0.0, |sample| sample);
    assert_eq!(output[..2], [0.4, -0.4]);
    let resumed = producer.snapshot();
    assert_eq!(resumed.consumed_frames, 4);
    assert_eq!(resumed.underrun_frames, second.underrun_frames);
    assert_eq!(resumed.underrun_callbacks, second.underrun_callbacks);
}

#[test]
fn eof_is_sampled_before_the_callback_for_underrun_accounting() {
    let (mut producer, mut consumer) = pcm_queue::<f32>(1, 2, false).expect("queue");
    producer.write_samples(&[0.1, 0.2]).expect("source");
    let mut output = [0.0; 4];
    consumer.render_mapped(&mut output, 0.0, |sample| {
        producer.close_input();
        sample
    });
    assert_eq!(output, [0.1, 0.2, 0.0, 0.0]);
    assert_eq!(producer.snapshot().underrun_frames, 2);
    assert_eq!(producer.snapshot().underrun_callbacks, 1);
    consumer.render_mapped(&mut output, 0.0, |sample| sample);
    assert_eq!(producer.snapshot().underrun_frames, 2);
    assert_eq!(producer.snapshot().underrun_callbacks, 1);
}

#[test]
fn unusable_capacity_and_an_abandoned_consumer_are_rejected() {
    assert!(pcm_queue::<f32>(0, 1, false).is_err());
    assert!(pcm_queue::<f32>(2, 0, false).is_err());
    assert!(pcm_queue::<f32>(2, usize::MAX, false).is_err());
    let (mut producer, consumer) = pcm_queue::<f32>(2, 1, false).expect("queue");
    drop(consumer);
    assert_eq!(
        producer
            .write_samples(&[0.1, 0.2])
            .expect_err("abandoned")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[test]
fn concurrent_handoff_keeps_exact_frame_order_without_losing_partial_writes() {
    let frames = 8_193;
    let source = (0..frames)
        .flat_map(|index| {
            let sample = 0.1 + index as f32 / frames as f32 * 0.7;
            [sample, -sample]
        })
        .collect::<Vec<_>>();
    let sent = source.clone();
    let (mut producer, mut consumer) = pcm_queue::<f32>(2, 127, false).expect("queue");
    let worker = thread::spawn(move || {
        for chunk in sent.chunks(373 * 2) {
            let mut pending = chunk;
            while !pending.is_empty() {
                match producer.write_samples(pending) {
                    Ok(count) => pending = &pending[count..],
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::yield_now(),
                    Err(error) => panic!("producer: {error}"),
                }
            }
        }
        producer.close_input();
        producer
    });
    let mut output = [0.0; 134];
    let mut received = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut iteration = 0;
    while received.len() < source.len() {
        assert!(Instant::now() < deadline, "handoff stalled");
        let samples = (iteration % 67 + 1) * 2;
        consumer.render_mapped(&mut output[..samples], f32::NAN, |sample| sample);
        for frame in output[..samples].as_chunks::<2>().0 {
            if frame[0].is_nan() {
                assert!(frame[1].is_nan());
            } else {
                assert!(frame[1].is_finite());
                received.extend_from_slice(frame);
            }
        }
        iteration += 1;
        thread::yield_now();
    }
    let producer = worker.join().expect("producer finishes");
    assert_eq!(received, source);
    assert!(producer.drained());
    assert_eq!(producer.snapshot().queued_frames, 0);
    assert_eq!(producer.snapshot().consumed_frames, frames as u64);
    let finished = producer.snapshot();
    consumer.render_mapped(&mut output, 0.0, |sample| sample);
    assert_eq!(
        producer.snapshot().underrun_frames,
        finished.underrun_frames
    );
    assert_eq!(
        producer.snapshot().underrun_callbacks,
        finished.underrun_callbacks
    );
}

#[test]
fn integer_frames_keep_source_values_and_channel_order_through_backpressure() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(2, 3, true).expect("queue");
    let source = [
        -8_388_608, 8_388_607, -32_768, 32_767, -1, 1, 256, -256, 0, 7,
    ];
    assert_eq!(producer.write_samples(&source).expect("prefix"), 6);
    assert_eq!(producer.snapshot().queued_frames, 3);
    assert_eq!(
        producer
            .write_samples(&source[6..])
            .expect_err("full")
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        producer
            .write_samples(&[1])
            .expect_err("partial frame")
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(producer.snapshot().queued_frames, 3);
    let mut first = [9; 4];
    consumer.render_mapped(&mut first, 0, |sample| sample);
    assert_eq!(first, [-8_388_608, 8_388_607, -32_768, 32_767]);
    assert_eq!(producer.write_samples(&source[6..]).expect("wrap"), 4);
    producer.close_input();
    let mut last = [9; 8];
    consumer.render_mapped(&mut last, 0, |sample| sample);
    assert_eq!(last, [-1, 1, 256, -256, 0, 7, 0, 0]);
    assert_eq!(producer.snapshot().consumed_frames, 5);
    assert!(!producer.snapshot().failed);
    assert!(producer.drained());
}

#[test]
fn strict_integer_startup_and_closed_tail_are_silent_without_failure() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(2, 2, true).expect("queue");
    let mut output = [9; 6];
    let mut calls = 0;
    consumer.render_mapped(&mut output, 0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(output, [0; 6]);
    assert_eq!(calls, 0);
    assert!(!producer.snapshot().failed);
    assert_eq!(producer.snapshot().underrun_callbacks, 0);
    producer.write_samples(&[-1, 1]).expect("source");
    consumer.render_mapped(&mut output[..0], 0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(calls, 0);
    assert_eq!(producer.snapshot().queued_frames, 1);
    producer.close_input();
    consumer.render_mapped(&mut output, 0, |sample| sample);
    assert_eq!(output, [-1, 1, 0, 0, 0, 0]);
    consumer.render_mapped(&mut output, 0, |sample| sample);
    assert_eq!(output, [0; 6]);
    assert!(producer.drained());
    assert!(!producer.snapshot().failed);
    assert_eq!(producer.snapshot().underrun_frames, 0);
}

#[test]
fn strict_callbacks_refuse_partial_frames_before_consuming_any_source() {
    for requested_samples in [1, 3] {
        let (mut producer, mut consumer) = pcm_queue::<i32>(2, 2, true).expect("queue");
        producer.write_samples(&[-1, 1, -2, 2]).expect("source");
        let mut output = [9; 4];
        let mut calls = 0;
        consumer.render_mapped(&mut output[..requested_samples], 0, |sample| {
            calls += 1;
            sample
        });
        assert_eq!(output[..requested_samples], vec![0; requested_samples]);
        assert_eq!(calls, 0);
        let failed = producer.snapshot();
        assert!(failed.failed);
        assert_eq!(failed.queued_frames, 2);
        assert_eq!(failed.consumed_frames, 0);
        assert_eq!(failed.underrun_frames, 0);
        assert_eq!(failed.underrun_callbacks, 0);
        assert_eq!(
            producer
                .write_samples(&[3, -3])
                .expect_err("failed queue")
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        producer.close_input();
        consumer.render_mapped(&mut output, 0, |sample| {
            calls += 1;
            sample
        });
        assert_eq!(output, [0; 4]);
        assert_eq!(calls, 0);
        assert_eq!(producer.snapshot(), failed);
    }
}

#[test]
fn strict_underrun_is_latched_and_prevents_automatic_resume() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(1, 3, true).expect("queue");
    producer.write_samples(&[-1, 1]).expect("source");
    let mut output = [9; 3];
    consumer.render_mapped(&mut output, 0, |sample| sample);
    assert_eq!(output, [-1, 1, 0]);
    let failed = producer.snapshot();
    assert!(failed.failed);
    assert_eq!(failed.consumed_frames, 2);
    assert_eq!(failed.underrun_frames, 1);
    assert_eq!(failed.underrun_callbacks, 1);
    assert_eq!(
        producer
            .write_samples(&[7])
            .expect_err("failed stream")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        producer
            .write_samples(&[])
            .expect_err("failed empty write")
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    producer.close_input();
    let mut calls = 0;
    consumer.render_mapped(&mut output, 0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(output, [0; 3]);
    assert_eq!(calls, 0);
    assert_eq!(producer.snapshot(), failed);
}

#[test]
fn strict_final_callback_accepts_a_producer_closing_during_copy() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(1, 2, true).expect("queue");
    producer.write_samples(&[-32_768, 32_767]).expect("source");
    let mut output = [9; 4];
    consumer.render_mapped(&mut output, 0, |sample| {
        producer.close_input();
        sample
    });
    assert_eq!(output, [-32_768, 32_767, 0, 0]);
    assert!(producer.drained());
    assert!(!producer.snapshot().failed);
    assert_eq!(producer.snapshot().underrun_callbacks, 0);
    assert_eq!(producer.snapshot().underrun_frames, 0);
}

#[test]
fn strict_close_does_not_excuse_missing_frames_when_source_remains_queued() {
    let (mut producer, mut consumer) = pcm_queue::<i32>(1, 3, true).expect("queue");
    producer.write_samples(&[1]).expect("first source frame");
    let mut output = [9; 3];
    consumer.render_mapped(&mut output, 0, |sample| {
        producer.write_samples(&[2, 3]).expect("last source frames");
        producer.close_input();
        sample
    });
    assert_eq!(output, [1, 0, 0]);
    let failed = producer.snapshot();
    assert!(failed.failed);
    assert_eq!(failed.consumed_frames, 1);
    assert_eq!(failed.queued_frames, 2);
    assert_eq!(failed.underrun_frames, 2);
    assert!(!producer.drained());
    let mut calls = 0;
    consumer.render_mapped(&mut output, 0, |sample| {
        calls += 1;
        sample
    });
    assert_eq!(output, [0; 3]);
    assert_eq!(calls, 0);
    assert_eq!(producer.snapshot(), failed);
}

#[test]
fn a_first_frame_published_before_its_counter_still_starts_strict_accounting() {
    use std::sync::atomic::Ordering;

    let (mut producer, mut consumer) = pcm_queue::<i32>(1, 2, true).expect("queue");
    // A real producer publishes its ring chunk immediately before updating
    // submitted_samples; the callback may observe this publication window.
    producer.ring.push(1).expect("published source");
    let mut output = [9; 2];
    consumer.render_mapped(&mut output, 0, |sample| sample);
    producer
        .counters
        .submitted_samples
        .fetch_add(1, Ordering::Release);
    assert_eq!(output, [1, 0]);
    let failed = producer.snapshot();
    assert!(failed.failed);
    assert_eq!(failed.consumed_frames, 1);
    assert_eq!(failed.underrun_frames, 1);
    assert_eq!(failed.underrun_callbacks, 1);
}

#[test]
fn concurrent_integer_handoff_preserves_low_bits_and_every_source_frame() {
    let frames = 8_193;
    let source = (0..frames)
        .flat_map(|index| [index, -index - 1])
        .collect::<Vec<i32>>();
    let sent = source.clone();
    let (mut producer, mut consumer) = pcm_queue::<i32>(2, 127, false).expect("queue");
    let worker = thread::spawn(move || {
        for chunk in sent.chunks(373 * 2) {
            let mut pending = chunk;
            while !pending.is_empty() {
                match producer.write_samples(pending) {
                    Ok(count) => pending = &pending[count..],
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::yield_now(),
                    Err(error) => panic!("producer: {error}"),
                }
            }
        }
        producer.close_input();
        producer
    });
    let mut output = [0; 134];
    let mut received = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut iteration = 0;
    while received.len() < source.len() {
        assert!(Instant::now() < deadline, "handoff stalled");
        let samples = (iteration % 67 + 1) * 2;
        consumer.render_mapped(&mut output[..samples], i32::MAX, |sample| sample);
        for frame in output[..samples].as_chunks::<2>().0 {
            if frame[0] == i32::MAX {
                assert_eq!(frame[1], i32::MAX);
            } else {
                received.extend_from_slice(frame);
            }
        }
        iteration += 1;
        thread::yield_now();
    }
    let producer = worker.join().expect("producer finishes");
    assert_eq!(received, source);
    assert!(producer.drained());
    assert_eq!(producer.snapshot().consumed_frames, frames as u64);
    assert!(!producer.snapshot().failed);
}
