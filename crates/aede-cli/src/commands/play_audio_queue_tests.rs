use std::io;
use std::thread;
use std::time::{Duration, Instant};

use super::pcm_queue;

fn f32le(samples: &[f32]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

#[test]
fn frame_aligned_prefixes_preserve_channel_order_through_wrap_and_retry() {
    let (mut producer, mut consumer) = pcm_queue(2, 3).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(2, 2).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(2, 1).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(2, 3).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(2, 2).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(2, 3).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(1, 2).expect("queue");
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
    assert!(pcm_queue(0, 1).is_err());
    assert!(pcm_queue(2, 0).is_err());
    assert!(pcm_queue(2, usize::MAX).is_err());
    let (mut producer, consumer) = pcm_queue(2, 1).expect("queue");
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
    let (mut producer, mut consumer) = pcm_queue(2, 127).expect("queue");
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
