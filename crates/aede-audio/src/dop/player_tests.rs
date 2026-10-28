use super::*;
use crate::{
    dop::{Carrier, driver::Driver, render::Renderer},
    dsd::test_fixtures::{dsf, source},
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize},
};
use std::time::Instant;

#[derive(Default)]
struct Trace {
    opened: AtomicUsize,
    closed: AtomicUsize,
    fail: AtomicBool,
    fail_replacement_close: AtomicBool,
    programme: Mutex<Vec<[u16; 2]>>,
    visits: Mutex<Vec<Arc<Signals>>>,
}
struct Fake {
    signals: Arc<Signals>,
    trace: Arc<Trace>,
    callback: Option<JoinHandle<()>>,
}
impl Driver for Fake {
    fn verify(&self) -> Result<(), String> {
        if self.trace.fail.load(Ordering::Acquire) {
            Err("device route changed".into())
        } else {
            Ok(())
        }
    }
    fn tail(&self) -> Duration {
        Duration::ZERO
    }
    fn close(&mut self) -> Result<(), String> {
        self.signals.cancel.store(true, Ordering::Release);
        if let Some(callback) = self.callback.take() {
            callback.join().map_err(|_| "fake callback failed")?;
            self.trace.closed.fetch_add(1, Ordering::AcqRel);
        }
        if self.trace.fail_replacement_close.load(Ordering::Acquire)
            && self.trace.opened.load(Ordering::Acquire) > 1
        {
            return Err("replacement DAC restoration failed".into());
        }
        Ok(())
    }
}
fn factory(trace: Arc<Trace>) -> Factory {
    Arc::new(move |format, consumer, signals| {
        signals.current()?;
        trace.visits.lock().unwrap().push(signals.clone());
        trace.opened.fetch_add(1, Ordering::AcqRel);
        let mut renderer = Renderer::new(consumer, format, Carrier::Packed24Le, signals.clone());
        let shared = signals.clone();
        let capture = trace.clone();
        let callback = std::thread::spawn(move || {
            let mut bytes = [0u8; 8 * 6];
            while !shared.cancel.load(Ordering::Acquire) {
                renderer.render(&mut bytes);
                // Capture is outside the actual renderer. Test-only recording
                // may allocate/lock; production HAL callbacks do neither.
                for frame in bytes.as_chunks::<6>().0 {
                    if matches!(frame[2], 5 | 0xfa) && frame[2] == frame[5] {
                        let words = [
                            u16::from_le_bytes([frame[0], frame[1]]),
                            u16::from_le_bytes([frame[3], frame[4]]),
                        ];
                        if words != [0x6969; 2] {
                            capture.programme.lock().unwrap().push(words);
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        Ok(Box::new(Fake {
            signals,
            trace: trace.clone(),
            callback: Some(callback),
        }))
    })
}

#[test]
fn seek_never_checks_retired_admission_after_replacement_audio_has_been_enabled() {
    let (source, _) = reference(4000, 2);
    let trace = Arc::new(Trace::default());
    let mut player = open(&source.0, trace.clone(), false, false);
    let late_checks = std::cell::Cell::new(0);
    let result = player.seek_frame_admitted(0, || {
        let visits = trace.visits.lock().unwrap();
        if visits.len() == 2 && visits[1].admitted.load(Ordering::Acquire) {
            late_checks.set(late_checks.get() + 1);
            return Err("owner retired at replacement publication".into());
        }
        Ok(())
    });
    assert_eq!(
        late_checks.get(),
        0,
        "a refused owner can already have emitted original bits"
    );
    result.unwrap();
    player.close().unwrap();
}

#[test]
fn failed_replacement_activation_retains_the_old_final_progress_instead_of_skipped_bits() {
    for route_fault in [false, true] {
        let (source, _) = reference(40_000, 2);
        let trace = Arc::new(Trace::default());
        let mut player = open(&source.0, trace.clone(), true, false);
        player.close().unwrap();
        let previous = player.snapshot().position_samples;
        let next = Player::prepare_held(
            player.retained.try_clone().unwrap(),
            player.factory.clone(),
            35_000,
            true,
            false,
            Some((player.source, player.stamp)),
            &|| Ok(()),
        )
        .unwrap();
        let pending = next.signals.clone();
        if route_fault {
            pending.fail_message(Fault::Route, "route failed after preparation".into());
        }
        let result = player.install(next, &|| {
            if route_fault {
                Ok(())
            } else {
                Err("owner retired before activation".into())
            }
        });
        assert!(result.is_err());
        assert_eq!(player.snapshot().position_samples, previous);
        assert!(!player.snapshot().completed);
        assert!(player.snapshot().error.is_some());
        assert!(!pending.admitted.load(Ordering::Acquire));
        assert_eq!(
            trace.opened.load(Ordering::Acquire),
            trace.closed.load(Ordering::Acquire)
        );
    }
}

#[test]
fn seek_retirement_keeps_replacement_restoration_failures_in_its_returned_error() {
    let mut refused_preparations = 0;
    for retire_at in 1..=16 {
        let (source, _) = reference(4000, 2);
        let trace = Arc::new(Trace::default());
        let mut player = open(&source.0, trace.clone(), true, false);
        trace.fail_replacement_close.store(true, Ordering::Release);
        let checks = std::cell::Cell::new(0);
        let result = player.seek_frame_admitted(100, || {
            checks.set(checks.get() + 1);
            if checks.get() == retire_at {
                Err("owner retired during seek".into())
            } else {
                Ok(())
            }
        });
        let installed = result.is_ok();
        if let Err(error) = result
            && trace.opened.load(Ordering::Acquire) == 2
        {
            refused_preparations += 1;
            assert!(error.contains("owner retired during seek"), "{error}");
            assert!(
                error.contains("replacement DAC restoration failed"),
                "{error}"
            );
            assert!(
                player
                    .snapshot()
                    .error
                    .unwrap()
                    .contains("restoration failed")
            );
            assert!(!player.snapshot().completed);
            assert_eq!(trace.closed.load(Ordering::Acquire), 2);
        }
        let cleanup = player.close();
        if installed {
            assert!(
                cleanup
                    .unwrap_err()
                    .contains("replacement DAC restoration failed")
            );
        } else {
            cleanup.unwrap();
        }
        assert!(trace.programme.lock().unwrap().is_empty());
    }
    assert!(refused_preparations > 0);
}
fn reference(frames: usize, channels: usize) -> (crate::test_support::Wave, Vec<[u16; 2]>) {
    let mut data = vec![Vec::new(); channels];
    let mut expected = Vec::new();
    for i in 0..frames {
        let first = (i % 1000 + 1) as u16;
        let words = [first, if channels == 1 { first } else { 0x8000 | first }];
        for channel in 0..channels {
            data[channel].extend(words[channel].to_be_bytes().map(u8::reverse_bits));
        }
        expected.push(words);
    }
    (
        source(&dsf(&data, 2_822_400, frames as u64 * 16, true)),
        expected,
    )
}
fn open(path: &Path, trace: Arc<Trace>, paused: bool, muted: bool) -> Player {
    Player::prepare(
        File::open(path).unwrap(),
        factory(trace),
        0,
        paused,
        muted,
        None,
        &|| Ok(()),
    )
    .unwrap()
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn real_source_bits_reach_the_dop_driver_without_filtering_and_mono_is_only_copied() {
    for channels in 1..=2 {
        let (source, expected) = reference(41, channels);
        let before = std::fs::read(&source.0).unwrap();
        let trace = Arc::new(Trace::default());
        let mut player = open(&source.0, trace.clone(), false, false);
        assert_eq!(player.info().channels, channels);
        assert_eq!(player.output_format().channels(), 2);
        until(|| player.snapshot().completed || player.snapshot().error.is_some());
        assert_eq!(player.snapshot().error, None);
        assert!(player.snapshot().completed);
        assert_eq!(player.snapshot().position_samples, 41 * 16);
        player.close().unwrap();
        assert_eq!(*trace.programme.lock().unwrap(), expected);
        assert_eq!(
            trace.opened.load(Ordering::Acquire),
            trace.closed.load(Ordering::Acquire)
        );
        assert_eq!(std::fs::read(&source.0).unwrap(), before);
    }
}

#[test]
fn paused_seek_preserves_the_original_descriptor_mute_and_source_position() {
    let (source, expected) = reference(241, 2);
    let backup = source.0.with_extension("retained-dsd");
    let trace = Arc::new(Trace::default());
    let mut player = open(&source.0, trace.clone(), true, true);
    std::fs::rename(&source.0, &backup).unwrap();
    let (replacement, _) = reference(641, 2);
    std::fs::copy(&replacement.0, &source.0).unwrap();
    player.seek_frame_admitted(17, || Ok(())).unwrap();
    let snapshot = player.snapshot();
    assert!(snapshot.paused && snapshot.muted);
    assert_eq!(
        (snapshot.position_samples, snapshot.source_samples),
        (17 * 16, 241 * 16)
    );
    assert!(trace.programme.lock().unwrap().is_empty());
    player.set_muted(false);
    player.resume();
    until(|| player.snapshot().completed || player.snapshot().error.is_some());
    player.close().unwrap();
    assert_eq!(*trace.programme.lock().unwrap(), expected[17..]);
    drop(player);
    std::fs::remove_file(backup).unwrap();
}

#[test]
fn route_failure_retires_output_and_can_never_be_labelled_complete_or_revived_by_seek() {
    let (source, _) = reference(4000, 2);
    let trace = Arc::new(Trace::default());
    let mut player = open(&source.0, trace.clone(), false, false);
    until(|| player.snapshot().position_samples > 0);
    trace.fail.store(true, Ordering::Release);
    until(|| player.snapshot().error.is_some());
    assert!(!player.snapshot().completed);
    assert!(player.seek_frame_admitted(0, || Ok(())).is_err());
    assert!(player.close().is_err());
    assert_eq!(
        trace.opened.load(Ordering::Acquire),
        trace.closed.load(Ordering::Acquire)
    );
}

#[test]
fn owner_retirement_during_preparation_closes_threads_before_any_original_bit_is_sent() {
    let (source, _) = reference(4000, 2);
    let trace = Arc::new(Trace::default());
    let checks = std::cell::Cell::new(0);
    let result = Player::prepare(
        File::open(&source.0).unwrap(),
        factory(trace.clone()),
        0,
        false,
        false,
        None,
        &|| {
            checks.set(checks.get() + 1);
            if checks.get() >= 3 {
                Err("owner retired".into())
            } else {
                Ok(())
            }
        },
    );
    assert!(result.is_err());
    assert!(trace.programme.lock().unwrap().is_empty());
    assert_eq!(
        trace.opened.load(Ordering::Acquire),
        trace.closed.load(Ordering::Acquire)
    );
}

#[test]
fn changed_source_is_refused_before_seek_opens_a_second_native_route() {
    let (source, _) = reference(41, 2);
    let trace = Arc::new(Trace::default());
    let mut player = open(&source.0, trace.clone(), true, false);
    // Let the source worker finish its small bounded read before editing it.
    until(|| player.signals.decoder_done.load(Ordering::Acquire));
    let writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&source.0)
        .unwrap();
    writer
        .set_len(std::fs::metadata(&source.0).unwrap().len() + 1)
        .unwrap();
    assert!(player.seek_frame_admitted(1, || Ok(())).is_err());
    assert!(player.snapshot().error.is_some());
    assert!(!player.snapshot().completed);
    assert!(player.seek_frame_admitted(0, || Ok(())).is_err());
    assert_eq!(trace.opened.load(Ordering::Acquire), 1);
    assert!(trace.programme.lock().unwrap().is_empty());
}

#[test]
fn rejected_seek_does_not_retire_the_current_paused_route() {
    let (source, _) = reference(41, 2);
    let trace = Arc::new(Trace::default());
    let mut player = open(&source.0, trace.clone(), true, false);
    assert!(player.seek_frame_admitted(42, || Ok(())).is_err());
    assert!(
        player
            .seek_frame_admitted(0, || Err("owner retired".into()))
            .is_err()
    );
    assert!(player.snapshot().paused);
    assert_eq!(player.snapshot().error, None);
    assert_eq!(trace.closed.load(Ordering::Acquire), 0);
    player.close().unwrap();
}

#[test]
fn explicitly_closed_player_cannot_restart_its_native_route() {
    let (source, _) = reference(41, 2);
    let trace = Arc::new(Trace::default());
    let mut player = open(&source.0, trace.clone(), true, false);
    player.close().unwrap();
    assert!(player.seek_frame_admitted(0, || Ok(())).is_err());
    assert_eq!(trace.opened.load(Ordering::Acquire), 1);
}

#[test]
fn initial_time_positions_are_bounded_and_rounded_to_original_dop_words() {
    let (source, _) = reference(41, 2);
    let player = open(&source.0, Arc::new(Trace::default()), true, false);
    let info = player.info();
    assert_eq!(frame_at_time(Duration::ZERO, info), 0);
    assert_eq!(frame_at_time(Duration::from_nanos(5_668), info), 0);
    assert_eq!(frame_at_time(Duration::from_nanos(5_669), info), 1);
    assert_eq!(frame_at_time(Duration::MAX, info), 41);
}
