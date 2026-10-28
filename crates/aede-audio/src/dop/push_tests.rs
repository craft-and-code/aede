use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Trace {
    limit: usize,
    bytes: Vec<u8>,
    starts: usize,
    closes: usize,
    fail: bool,
    retire_on_space: Option<(Arc<Signals>, bool)>,
    retire_on_write: Option<(Arc<Signals>, bool)>,
}
struct Device(Rc<RefCell<Trace>>);
impl Sink for Device {
    fn verify(&self) -> Result<(), String> {
        Ok(())
    }
    fn space(&self) -> Result<usize, String> {
        if let Some((signals, cancelled)) = self.0.borrow_mut().retire_on_space.take() {
            if cancelled {
                signals.cancel.store(true, Ordering::Release);
            } else {
                signals.fail(super::super::state::Fault::Route);
            }
        }
        Ok(4)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<usize, String> {
        let mut trace = self.0.borrow_mut();
        if trace.fail {
            return Err("native device disconnected".into());
        }
        let frames = trace.limit.min(bytes.len() / 6);
        trace.bytes.extend_from_slice(&bytes[..frames * 6]);
        if let Some((signals, cancelled)) = trace.retire_on_write.take() {
            if cancelled {
                signals.cancel.store(true, Ordering::Release);
            } else {
                signals.fail(super::super::state::Fault::Route);
            }
        }
        Ok(frames)
    }
    fn start(&mut self) -> Result<(), String> {
        self.0.borrow_mut().starts += 1;
        Ok(())
    }
    fn close(&mut self) -> Result<(), String> {
        self.0.borrow_mut().closes += 1;
        Ok(())
    }
    fn tail(&self) -> Duration {
        Duration::ZERO
    }
}

#[test]
fn retirement_during_native_space_query_cannot_submit_pending_original_words() {
    for cancelled in [false, true] {
        let (mut driver, signals, trace) = prepared();
        trace.borrow_mut().limit = 0;
        driver.service().unwrap(); // original words staged, none accepted
        let bytes_before = trace.borrow().bytes.len();
        {
            let mut trace = trace.borrow_mut();
            trace.limit = 4;
            trace.retire_on_space = Some((signals.clone(), cancelled));
        }
        assert!(driver.service().is_err());
        assert_eq!(trace.borrow().bytes.len(), bytes_before);
        assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
        assert!(!signals.source_finished.load(Ordering::Acquire));
    }
}

#[test]
fn an_inflight_write_keeps_its_actual_accepted_prefix_when_retirement_arrives() {
    for cancelled in [false, true] {
        let (mut driver, signals, trace) = prepared();
        {
            let mut trace = trace.borrow_mut();
            trace.limit = 1;
            trace.retire_on_write = Some((signals.clone(), cancelled));
        }
        driver.service().unwrap();
        assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
        assert!(!signals.source_finished.load(Ordering::Acquire));
        let bytes_after = trace.borrow().bytes.len();
        assert!(driver.service().is_err());
        assert_eq!(trace.borrow().bytes.len(), bytes_after);
        assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    }
}
fn prepared() -> (Push<Device>, Arc<Signals>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace {
        limit: 4,
        ..Trace::default()
    }));
    let (mut producer, consumer) = crate::queue::pcm_queue(2, 8, true).unwrap();
    producer
        .write_samples(&[
            Payload(0x1234),
            Payload(0xabcd),
            Payload(0x5678),
            Payload(0x7654),
        ])
        .unwrap();
    producer.close_input();
    let signals = Arc::new(Signals::new(2, false, false));
    signals.ready.store(true, Ordering::Release);
    let mut driver = Push::new(
        Device(trace.clone()),
        Format::new(2_822_400, 2).unwrap(),
        Carrier::Packed24Le,
        consumer,
        signals.clone(),
        4,
    )
    .unwrap();
    for _ in 0..8 {
        driver.service().unwrap();
    }
    assert!(driver.prepared());
    assert_eq!(trace.borrow().starts, 1);
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    signals.admitted.store(true, Ordering::Release);
    (driver, signals, trace)
}
#[test]
fn partial_native_writes_keep_every_payload_and_marker_without_advancing_unaccepted_progress() {
    let (mut driver, signals, trace) = prepared();
    trace.borrow_mut().limit = 0;
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    assert!(!signals.source_finished.load(Ordering::Acquire));
    trace.borrow_mut().limit = 1;
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    assert!(!signals.source_finished.load(Ordering::Acquire));
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 2);
    assert!(signals.source_finished.load(Ordering::Acquire));
    let bytes = &trace.borrow().bytes;
    assert_eq!(
        &bytes[32 * 6..],
        &[
            0x34, 0x12, 5, 0xcd, 0xab, 5, 0x78, 0x56, 0xfa, 0x54, 0x76, 0xfa
        ]
    );
    for (i, frame) in bytes.as_chunks::<6>().0.iter().enumerate() {
        let marker = if i.is_multiple_of(2) { 5 } else { 0xfa };
        assert_eq!((frame[2], frame[5]), (marker, marker));
    }
}
#[test]
fn a_native_failure_after_staging_does_not_claim_delivery_and_close_is_idempotent() {
    let (mut driver, signals, trace) = prepared();
    trace.borrow_mut().limit = 0;
    driver.service().unwrap();
    trace.borrow_mut().fail = true;
    assert!(driver.service().is_err());
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    assert!(!signals.source_finished.load(Ordering::Acquire));
    driver.close().unwrap();
    driver.close().unwrap();
    assert_eq!(trace.borrow().closes, 1);
}
#[test]
fn paused_push_output_preserves_source_words_and_muting_consumes_only_accepted_words() {
    let (mut driver, signals, trace) = prepared();
    signals.paused.store(true, Ordering::Release);
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    signals.paused.store(false, Ordering::Release);
    signals.muted.store(true, Ordering::Release);
    trace.borrow_mut().limit = 1;
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    let trace = trace.borrow();
    assert_eq!(
        &trace.bytes[trace.bytes.len() - 6..],
        &[0x69, 0x69, 5, 0x69, 0x69, 5]
    );
}

#[test]
fn pause_and_mute_changes_apply_to_unaccepted_staged_words_without_changing_marker_phase() {
    let (mut driver, signals, trace) = prepared();
    trace.borrow_mut().limit = 0;
    driver.service().unwrap(); // source words staged, none accepted
    signals.paused.store(true, Ordering::Release);
    trace.borrow_mut().limit = 3;
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 0);
    assert_eq!(
        &trace.borrow().bytes[32 * 6..],
        &[
            0x69, 0x69, 5, 0x69, 0x69, 5, 0x69, 0x69, 0xfa, 0x69, 0x69, 0xfa, 0x69, 0x69, 5, 0x69,
            0x69, 5
        ]
    );
    signals.paused.store(false, Ordering::Release);
    signals.muted.store(true, Ordering::Release);
    trace.borrow_mut().limit = 1;
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 1);
    signals.muted.store(false, Ordering::Release);
    driver.service().unwrap();
    assert_eq!(signals.consumed.load(Ordering::Acquire), 2);
    let bytes = &trace.borrow().bytes;
    assert_eq!(&bytes[bytes.len() - 6..], &[0x78, 0x56, 5, 0x54, 0x76, 5]);
    for (i, frame) in bytes.as_chunks::<6>().0.iter().enumerate() {
        let marker = if i.is_multiple_of(2) { 5 } else { 0xfa };
        assert_eq!((frame[2], frame[5]), (marker, marker));
    }
}
