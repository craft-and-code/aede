use super::*;

struct RetiredService {
    signals: Arc<Signals>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}
impl Driver for RetiredService {
    fn verify(&self) -> Result<(), String> {
        Ok(())
    }
    fn service(&mut self) -> Result<(), ServiceError> {
        self.signals.cancel.store(true, Ordering::Release);
        Err(ServiceError::Retired)
    }
    fn tail(&self) -> Duration {
        Duration::ZERO
    }
    fn close(&mut self) -> Result<(), String> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}

#[test]
fn cancellation_detected_inside_a_native_service_remains_a_normal_stop() {
    let signals = Arc::new(Signals::new(1, false, false));
    signals.ready.store(true, Ordering::Release);
    let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let capture = closed.clone();
    let factory: Factory = Arc::new(move |_, _, signals| {
        Ok(Box::new(RetiredService {
            signals,
            closed: capture.clone(),
        }))
    });
    let (_producer, consumer) = crate::queue::pcm_queue(2, 2, true).unwrap();
    let (send, _receive) = std::sync::mpsc::sync_channel(1);
    run(
        factory,
        Format::new(2_822_400, 2).unwrap(),
        consumer,
        signals.clone(),
        send,
    )
    .unwrap();
    assert!(closed.load(Ordering::Acquire));
    assert!(signals.stopped.load(Ordering::Acquire));
    assert!(!signals.completed.load(Ordering::Acquire));
    assert_eq!(signals.error(), None);
}

#[test]
fn stalled_native_output_has_a_progress_deadline_that_excludes_pauses_and_held_setup() {
    let mut progress = Progress::default();
    progress.observe(Duration::from_secs(20), 0, false).unwrap();
    progress.observe(Duration::from_secs(24), 0, true).unwrap();
    assert!(progress.observe(Duration::from_secs(25), 0, true).is_err());
    progress
        .observe(Duration::from_secs(100), 0, false)
        .unwrap();
    progress.observe(Duration::from_secs(104), 0, true).unwrap();
    progress
        .observe(Duration::from_secs(105), 512, true)
        .unwrap();
    progress
        .observe(Duration::from_secs(109), 512, true)
        .unwrap();
    assert!(
        progress
            .observe(Duration::from_secs(110), 512, true)
            .is_err()
    );
}

struct LateFault(Arc<Signals>);
impl Driver for LateFault {
    fn verify(&self) -> Result<(), String> {
        Ok(())
    }
    fn tail(&self) -> Duration {
        Duration::ZERO
    }
    fn close(&mut self) -> Result<(), String> {
        self.0.fail(Fault::Source);
        Ok(())
    }
}
#[test]
fn a_source_fault_racing_final_native_cleanup_can_never_seal_a_completed_visit() {
    let signals = Arc::new(Signals::new(1, false, false));
    signals.ready.store(true, Ordering::Release);
    signals.source_finished.store(true, Ordering::Release);
    signals.decoder_done.store(true, Ordering::Release);
    let factory: Factory = Arc::new(|_, _, signals| Ok(Box::new(LateFault(signals))));
    let (_producer, consumer) = crate::queue::pcm_queue(2, 2, true).unwrap();
    let (send, _receive) = std::sync::mpsc::sync_channel(1);
    assert!(
        run(
            factory,
            Format::new(2_822_400, 2).unwrap(),
            consumer,
            signals.clone(),
            send
        )
        .is_err()
    );
    assert!(!signals.completed.load(Ordering::Acquire));
    assert!(signals.error().is_some());
}
