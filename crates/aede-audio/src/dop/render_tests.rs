use super::*;
use crate::queue::pcm_queue;

fn renderer(expected: u64) -> (crate::queue::PcmProducer<Payload>, Renderer, Arc<Signals>) {
    let format = Format::new(2_822_400, 2).unwrap();
    let (producer, consumer) = pcm_queue::<Payload>(2, 8, true).unwrap();
    let signals = Arc::new(Signals::new(expected, false, false));
    signals.ready.store(true, Ordering::Release);
    signals.admitted.store(true, Ordering::Release);
    (
        producer,
        Renderer::new(consumer, format, Carrier::Packed24Le, signals.clone()),
        signals,
    )
}
fn words(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<3>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect()
}
fn markers(bytes: &[u8], first_frame: usize) {
    for (index, frame) in bytes.as_chunks::<6>().0.iter().enumerate() {
        let marker = if (index + first_frame).is_multiple_of(2) {
            5
        } else {
            0xfa
        };
        assert_eq!((frame[2], frame[5]), (marker, marker));
    }
}

#[test]
fn retirement_after_queue_removal_cannot_count_an_unrendered_frame_as_delivered() {
    for cancelled in [false, true] {
        let (mut producer, mut renderer, signals) = renderer(1);
        producer
            .write_samples(&[Payload(0x1234), Payload(0xabcd)])
            .unwrap();
        renderer.render(&mut [0; 32 * 6]);
        // Interleave retirement between render's queue removal and encoding.
        // These words have left the ring but have not reached a HAL buffer.
        assert_eq!(
            renderer.next_frame(true, false),
            [Payload(0x1234), Payload(0xabcd)]
        );
        if cancelled {
            signals.cancel.store(true, Ordering::Release);
        } else {
            signals.fail(Fault::Source);
        }
        let mut output = [0xff; 6];
        renderer.render(&mut output);
        assert_eq!(output, [0; 6]);
        assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
        assert!(!signals.source_finished.load(Ordering::Acquire));
    }
}

#[test]
fn invalid_push_geometry_retires_staging_without_consuming_source() {
    let (mut producer, mut renderer, signals) = renderer(1);
    producer.write_samples(&[Payload(1), Payload(2)]).unwrap();
    assert!(renderer.stage(&mut [[Payload(0); 2]], &mut [0]).is_err());
    assert_eq!(signals.fault.load(Ordering::Acquire), Fault::Buffer as u8);
    assert!(renderer.stage(&mut [[Payload(0); 2]], &mut [0, 0]).is_err());
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
}

#[test]
fn preamble_pause_resume_and_eof_keep_continuous_markers_without_dropping_source() {
    let (mut producer, mut renderer, signals) = renderer(3);
    producer
        .write_samples(&[
            Payload(1),
            Payload(2),
            Payload(3),
            Payload(4),
            Payload(5),
            Payload(6),
        ])
        .unwrap();
    producer.close_input();
    let mut first = vec![0; 33 * 6];
    renderer.render(&mut first);
    markers(&first, 0);
    assert_eq!(words(&first[..32 * 6]), vec![0x6969; 64]);
    assert_eq!(words(&first[32 * 6..]), [1, 2]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    signals.paused.store(true, Ordering::Release);
    let mut pause = [0; 6];
    renderer.render(&mut pause);
    markers(&pause, 33);
    assert_eq!(words(&pause), [0x6969, 0x6969]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    signals.paused.store(false, Ordering::Release);
    let mut final_block = [0; 18];
    renderer.render(&mut final_block);
    markers(&final_block, 34);
    assert_eq!(words(&final_block), [3, 4, 5, 6, 0x6969, 0x6969]);
    assert!(signals.source_finished.load(Ordering::Acquire));
    assert_eq!(signals.fault.load(Ordering::Acquire), 0);
}

#[test]
fn muted_dsd_keeps_time_and_source_order_without_a_pcm_gain_operation() {
    let (mut producer, mut renderer, signals) = renderer(2);
    producer
        .write_samples(&[Payload(1), Payload(2), Payload(3), Payload(4)])
        .unwrap();
    let mut preamble = [0; 32 * 6];
    renderer.render(&mut preamble);
    signals.muted.store(true, Ordering::Release);
    let mut frame = [0; 6];
    renderer.render(&mut frame);
    assert_eq!(words(&frame), [0x6969, 0x6969]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    signals.muted.store(false, Ordering::Release);
    renderer.render(&mut frame);
    assert_eq!(words(&frame), [3, 4]);
}

#[test]
fn underrun_retires_output_instead_of_hiding_a_gap_or_reporting_completion() {
    let (mut producer, mut renderer, signals) = renderer(2);
    producer.write_samples(&[Payload(1), Payload(2)]).unwrap();
    let mut bytes = [0; 34 * 6];
    renderer.render(&mut bytes);
    assert_eq!(signals.fault.load(Ordering::Acquire), Fault::Underrun as u8);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    assert!(!signals.source_finished.load(Ordering::Acquire));
    producer.write_samples(&[Payload(3), Payload(4)]).unwrap();
    let mut after = [99; 6];
    renderer.render(&mut after);
    assert_eq!(after, [0; 6]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
}

#[test]
fn held_and_invalid_callbacks_never_consume_source_or_shift_channels() {
    let (mut producer, mut renderer, signals) = renderer(2);
    producer.write_samples(&[Payload(1), Payload(2)]).unwrap();
    signals.admitted.store(false, Ordering::Release);
    let mut held = [0; 34 * 6];
    renderer.render(&mut held);
    markers(&held, 0);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    let mut partial = [42; 5];
    renderer.render(&mut partial);
    assert_eq!(partial, [0; 5]);
    assert_eq!(signals.fault.load(Ordering::Acquire), Fault::Buffer as u8);
}
