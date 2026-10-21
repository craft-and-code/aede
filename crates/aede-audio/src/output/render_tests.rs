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
