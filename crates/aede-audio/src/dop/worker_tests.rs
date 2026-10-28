use super::*;
use crate::{
    dop::{Carrier, Format, render::Renderer},
    dsd::test_fixtures::{dsf, source},
    queue::pcm_queue,
};

#[test]
fn a_first_read_burst_cannot_admit_audio_before_the_source_queue_has_prefilled() {
    let (mut producer, consumer) = pcm_queue(2, 8, true).unwrap();
    let signals = Arc::new(Signals::new(8, false, false));
    producer.write_samples(&[Payload(0x1234); 4]).unwrap();
    publish_ready(&producer, &signals);
    assert!(!signals.ready.load(Ordering::Acquire));

    producer.write_samples(&[Payload(0x1234); 12]).unwrap();
    publish_ready(&producer, &signals);
    assert!(signals.ready.load(Ordering::Acquire));
    signals.admitted.store(true, Ordering::Release);
    let mut renderer = Renderer::new(
        consumer,
        Format::new(2_822_400, 2).unwrap(),
        Carrier::Packed24Le,
        signals.clone(),
    );
    renderer.render(&mut [0; (32 + 8) * 6]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 8);
    assert_eq!(signals.fault.load(Ordering::Acquire), 0);
}

#[test]
fn complete_short_sources_are_ready_without_waiting_for_a_full_queue() {
    let file = source(&dsf(&[vec![0x96; 10], vec![0x96; 10]], 2_822_400, 80, true));
    let reader = Reader::open(&file.0).unwrap();
    let (producer, consumer) = pcm_queue(2, 16, true).unwrap();
    let signals = Arc::new(Signals::new(5, false, false));
    run(reader, producer, signals.clone());
    assert!(signals.ready.load(Ordering::Acquire));
    assert!(signals.decoder_done.load(Ordering::Acquire));
    assert_eq!(signals.fault.load(Ordering::Acquire), 0);
    assert_eq!(consumer.available_frames(), 5);
    assert!(consumer.input_closed());
}
