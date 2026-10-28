use super::*;
use crate::dop::{Carrier, Format, render::Renderer, state::Signals};
use std::sync::Arc;

fn client(tag: u64) -> (Box<Client>, std::sync::Weak<Signals>) {
    let signals = Arc::new(Signals::new(tag, false, false));
    let weak = Arc::downgrade(&signals);
    let (_producer, consumer) = crate::queue::pcm_queue(2, 2, true).unwrap();
    let renderer = Renderer::new(
        consumer,
        Format::new(2_822_400, 2).unwrap(),
        Carrier::Packed24Le,
        signals.clone(),
    );
    (Box::new(Client::new(renderer, signals, 512)), weak)
}

#[test]
fn retired_hal_tokens_cannot_access_replacement_clients_or_retain_source_storage() {
    let registry = Registry::new();
    let (first, weak) = client(7);
    let mut context = Context::new(&registry, first).unwrap();
    let old = context.token();
    assert_eq!(registry.visit(old, |c| c.signals.expected), Some(7));
    assert!(context.retire());
    assert!(weak.upgrade().is_none());
    let (next, _) = client(19);
    let context = Context::new(&registry, next).unwrap();
    assert_ne!(old, context.token());
    assert_eq!(registry.visit(old, |c| c.signals.expected), None);
    assert_eq!(
        registry.visit(context.token(), |c| c.signals.expected),
        Some(19)
    );
}

#[test]
fn late_callbacks_racing_slot_reuse_never_observe_another_sessions_state() {
    let registry = Arc::new(Registry::new());
    let (first, _) = client(7);
    let mut context = Context::new(&registry, first).unwrap();
    let old = context.token().addr();
    assert!(context.retire());
    let stop = Arc::new(AtomicBool::new(false));
    let check = registry.clone();
    let done = stop.clone();
    let callback = std::thread::spawn(move || {
        while !done.load(Ordering::Acquire) {
            if let Some(value) =
                check.visit(ptr::without_provenance_mut(old), |c| c.signals.expected)
            {
                assert_eq!(value, 7);
            }
            std::thread::yield_now();
        }
    });
    for value in 10..1010 {
        let (client, _) = client(value);
        let context = Context::new(&registry, client).unwrap();
        assert_eq!(
            registry.visit(context.token(), |c| c.signals.expected),
            Some(value)
        );
        assert_eq!(
            registry.visit(ptr::without_provenance_mut(old), |c| c.signals.expected),
            None
        );
    }
    stop.store(true, Ordering::Release);
    callback.join().unwrap();
}

#[test]
fn callback_registry_capacity_is_bounded_and_reclaimed_after_quiescence() {
    let registry = Registry::new();
    let mut handles = Vec::new();
    for value in 0..64 {
        handles.push(Context::new(&registry, client(value).0).unwrap());
    }
    assert!(Context::new(&registry, client(100).0).is_err());
    handles.pop();
    assert!(Context::new(&registry, client(101).0).is_ok());
}
