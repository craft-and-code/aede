use super::*;
use crate::dop::{Carrier, coreaudio::api::AudioBuffer};
use std::mem::{offset_of, size_of};

#[test]
fn hal_layouts_match_the_public_64_bit_sdk_abi() {
    assert_eq!(size_of::<super::super::format::Description>(), 40);
    assert_eq!(size_of::<super::super::format::Ranged>(), 56);
    assert_eq!(size_of::<Property>(), 12);
    assert_eq!(size_of::<AudioBuffer>(), 16);
    assert_eq!(size_of::<BufferList>(), 24);
    assert_eq!(offset_of!(BufferList, first), 8);
}

#[test]
fn invalid_and_retired_hal_buffers_never_publish_or_consume_programme_bits() {
    let (mut producer, consumer) = crate::queue::pcm_queue(2, 2, true).unwrap();
    producer
        .write_samples(&[Payload(0x1234), Payload(0xabcd)])
        .unwrap();
    let signals = Arc::new(Signals::new(1, false, false));
    signals.admitted.store(true, Ordering::Release);
    signals.ready.store(true, Ordering::Release);
    let renderer = Renderer::new(
        consumer,
        Format::new(2_822_400, 2).unwrap(),
        Carrier::Packed24Le,
        signals.clone(),
    );
    let client = Client::new(renderer, signals.clone(), 8);
    let mut bytes = [0x55u8; 48];
    let mut list = BufferList {
        count: 1,
        first: AudioBuffer {
            channels: 1,
            size: 48,
            data: bytes.as_mut_ptr().cast(),
        },
    };
    // SAFETY: this test owns the complete buffer list and writable backing array.
    assert_eq!(unsafe { render_client(&client, &mut list) }, -50);
    assert_eq!(bytes, [0; 48]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    assert!(signals.error().is_some());
    signals.cancel.store(true, Ordering::Release);
    list.first.channels = 2;
    bytes.fill(0x55);
    // SAFETY: the same exclusive test-owned storage, with a valid frame shape.
    assert_eq!(unsafe { render_client(&client, &mut list) }, 0);
    assert_eq!(bytes, [0; 48]);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    bytes.fill(0x55);
    // SAFETY: late-token clearing touches only the valid HAL-shaped test buffer.
    assert_eq!(unsafe { clear_output(&mut list) }, 0);
    assert_eq!(bytes, [0; 48]);
}
