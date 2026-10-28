use super::*;
use std::sync::atomic::AtomicUsize;
fn held(channels: usize, samples: u64, position: u64) -> (StreamOutput, Arc<AtomicUsize>) {
    let opened = Arc::new(AtomicUsize::new(0));
    let trace = opened.clone();
    let factory: Factory = Arc::new(move |_, _, _| {
        trace.fetch_add(1, Ordering::AcqRel);
        Err("unexpected device open".into())
    });
    (
        StreamOutput::with_factory(
            Format::new(2_822_400, channels).unwrap(),
            samples,
            position,
            factory,
        )
        .unwrap(),
        opened,
    )
}
#[test]
fn a_short_packet_does_not_open_a_device_or_admit_incomplete_eof() {
    let (mut output, opened) = held(2, 160, 0);
    assert_eq!(output.write(&[Payload(1), Payload(2)]).unwrap(), 2);
    assert!(!output.poll_admitted(|| Ok(())).unwrap().completed);
    assert_eq!(opened.load(Ordering::Acquire), 0);
    assert!(output.finish().is_err());
    assert!(output.write(&[Payload(1), Payload(2)]).is_err());
    assert!(output.snapshot().error.is_some());
    assert_eq!(opened.load(Ordering::Acquire), 0);
}
#[test]
fn eof_seek_requires_no_device_and_keeps_the_original_bit_position() {
    let (mut output, opened) = held(1, 17, 17);
    output.finish().unwrap();
    assert!(output.poll_admitted(|| Ok(())).unwrap().completed);
    assert_eq!(output.snapshot().position_samples, 17);
    assert_eq!(output.consumed_frames(), 0);
    assert_eq!(opened.load(Ordering::Acquire), 0);
    output.close().unwrap();
}
#[test]
fn malformed_packets_are_terminal_before_any_valid_prefix_is_published() {
    for words in [
        vec![],
        vec![Payload(1); 3],
        vec![Payload(1); 10],
        vec![Payload(1); MAX_WORDS + 2],
    ] {
        let (mut output, opened) = held(2, 64, 0);
        assert!(output.write(&words).is_err());
        assert!(output.finish().is_err());
        assert!(output.close().is_err());
        assert_eq!(output.supplied, 0);
        assert_eq!(opened.load(Ordering::Acquire), 0);
    }
}

use crate::dop::{
    Carrier,
    driver::{Driver, ServiceError},
    render::Renderer,
};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Trace {
    opened: AtomicUsize,
    closed: AtomicUsize,
    restore_fail: std::sync::atomic::AtomicBool,
    words: Mutex<Vec<[u16; 2]>>,
}
struct Fake {
    render: Renderer,
    signals: Arc<Signals>,
    trace: Arc<Trace>,
}
impl Driver for Fake {
    fn service(&mut self) -> Result<(), ServiceError> {
        self.signals.current()?;
        let mut bytes = [0u8; 24];
        self.render.render(&mut bytes);
        for f in bytes.as_chunks::<6>().0 {
            if matches!(f[2], 5 | 0xfa) && f[2] == f[5] {
                let w = [
                    u16::from_le_bytes([f[0], f[1]]),
                    u16::from_le_bytes([f[3], f[4]]),
                ];
                if w != [0x6969; 2] {
                    self.trace.words.lock().unwrap().push(w);
                }
            }
        }
        Ok(())
    }
    fn verify(&self) -> Result<(), String> {
        Ok(())
    }
    fn tail(&self) -> Duration {
        Duration::ZERO
    }
    fn close(&mut self) -> Result<(), String> {
        self.trace.closed.fetch_add(1, Ordering::AcqRel);
        if self.trace.restore_fail.load(Ordering::Acquire) {
            Err("DAC restoration failed".into())
        } else {
            Ok(())
        }
    }
}
fn native(rate: u32, channels: usize, samples: u64) -> (StreamOutput, Arc<Trace>) {
    let trace = Arc::new(Trace::default());
    let owned = trace.clone();
    let factory: Factory = Arc::new(move |format, input, signals| {
        owned.opened.fetch_add(1, Ordering::AcqRel);
        Ok(Box::new(Fake {
            render: Renderer::new(input, format, Carrier::Packed24Le, signals.clone()),
            signals,
            trace: owned.clone(),
        }))
    });
    (
        StreamOutput::with_factory(Format::new(rate, channels).unwrap(), samples, 0, factory)
            .unwrap(),
        trace,
    )
}
fn complete(output: &mut StreamOutput) -> Snapshot {
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let s = output.poll_admitted(|| Ok(())).unwrap();
        if s.completed {
            return s;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn original_words_survive_both_clocks_channel_counts_and_partial_tail_progress() {
    for rate in [2_822_400, 5_644_800] {
        for channels in [1, 2] {
            let (mut output, trace) = native(rate, channels, 149);
            let words: Vec<_> = (0..10)
                .flat_map(|n| (0..channels).map(move |c| Payload(0x1200 + n * 2 + c as u16)))
                .collect();
            assert_eq!(output.write(&words).unwrap(), words.len());
            assert_eq!(trace.opened.load(Ordering::Acquire), 0);
            output.finish().unwrap();
            assert_eq!(complete(&mut output).position_samples, 149);
            assert_eq!(output.consumed_frames(), 10);
            assert!(
                output.poll_admitted(|| Ok(())).unwrap().completed,
                "natural driver retirement stays completed"
            );
            let expected: Vec<_> = words
                .chunks_exact(channels)
                .map(|f| [f[0].0, f[channels - 1].0])
                .collect();
            assert_eq!(*trace.words.lock().unwrap(), expected);
            output.close().unwrap();
            assert_eq!(trace.closed.load(Ordering::Acquire), 1);
        }
    }
}
#[test]
fn pause_and_initial_mute_use_silence_without_losing_original_time() {
    for muted in [false, true] {
        let (mut output, trace) = native(2_822_400, 1, 149);
        output.pause();
        output.set_muted(muted);
        output.write(&[Payload(0x1234); 10]).unwrap();
        output.finish().unwrap();
        for _ in 0..30 {
            output.poll_admitted(|| Ok(())).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(output.consumed_frames(), 0);
        assert!(trace.words.lock().unwrap().is_empty());
        output.resume();
        assert!(complete(&mut output).completed);
        assert_eq!(trace.words.lock().unwrap().is_empty(), muted);
        output.close().unwrap();
    }
}
#[test]
fn caller_retirement_during_native_preparation_never_admits_programme_bits() {
    let (mut output, trace) = native(2_822_400, 1, 149);
    output.write(&[Payload(0x1234); 10]).unwrap();
    output.finish().unwrap();
    let until = Instant::now() + Duration::from_secs(1);
    loop {
        if output
            .poll_admitted(|| {
                if trace.opened.load(Ordering::Acquire) > 0 {
                    Err("owner retired".into())
                } else {
                    Ok(())
                }
            })
            .is_err()
        {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(trace.words.lock().unwrap().is_empty());
    assert!(!output.snapshot().completed);
    assert!(output.snapshot().error.is_some());
    assert_eq!(
        trace.opened.load(Ordering::Acquire),
        trace.closed.load(Ordering::Acquire)
    );
}
#[test]
fn restoration_failure_cannot_become_success_on_repeated_close() {
    let (mut output, trace) = native(2_822_400, 1, 16);
    trace.restore_fail.store(true, Ordering::Release);
    output.write(&[Payload(0x1234)]).unwrap();
    output.finish().unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        if output.poll_admitted(|| Ok(())).is_err() {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!output.snapshot().completed);
    assert!(output.close().unwrap_err().contains("restoration"));
    assert!(output.close().unwrap_err().contains("restoration"));
}
#[test]
fn full_prefill_backpressures_without_discarding_original_frames() {
    let capacity = 176_400usize / 2;
    let (mut output, trace) = native(2_822_400, 2, (capacity as u64 + 8) * 16);
    let words = vec![Payload(0x1234); MAX_WORDS];
    let mut supplied = 0;
    loop {
        let remaining = (capacity + 8 - supplied) * 2;
        let n = output.write(&words[..words.len().min(remaining)]).unwrap() / 2;
        if n == 0 {
            break;
        }
        supplied += n;
    }
    assert_eq!(supplied, capacity);
    assert_eq!(trace.opened.load(Ordering::Acquire), 0);
    let until = Instant::now() + Duration::from_secs(1);
    while !output.signals.admitted.load(Ordering::Acquire) {
        output.poll_admitted(|| Ok(())).unwrap();
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    output.close().unwrap();
    assert_eq!(trace.closed.load(Ordering::Acquire), 1);
    assert!(!output.snapshot().completed);
}
