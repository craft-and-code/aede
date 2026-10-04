use std::cell::Cell;
use std::io;
use std::rc::Rc;

use super::*;

struct FakeOutput {
    time: Rc<Cell<u64>>,
    format: PcmFormat,
    queued_until: u64,
    progress: bool,
    progress_interval: u64,
    native: bool,
    negotiated: Option<PcmFormat>,
    closed: bool,
    finalized: usize,
    reopened: usize,
    error_at: Option<u64>,
    pause_calls: Cell<usize>,
    resume_calls: Cell<usize>,
}

impl FakeOutput {
    fn new(time: &Rc<Cell<u64>>) -> Self {
        Self {
            time: Rc::clone(time),
            format: PcmFormat::new(48_000, 2).expect("format"),
            queued_until: 0,
            progress: true,
            progress_interval: 1,
            native: true,
            negotiated: None,
            closed: false,
            finalized: 0,
            reopened: 0,
            error_at: None,
            pause_calls: Cell::new(0),
            resume_calls: Cell::new(0),
        }
    }
}

impl io::Write for FakeOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl PlaybackOutput for FakeOutput {
    fn pause(&self) -> Res {
        self.pause_calls.set(self.pause_calls.get() + 1);
        Ok(())
    }
    fn resume(&self) -> Res {
        self.resume_calls.set(self.resume_calls.get() + 1);
        Ok(())
    }
}

impl SessionOutput for FakeOutput {
    fn needs_reopen(&mut self, format: PcmFormat) -> Result<bool, Box<dyn Error>> {
        Ok(self.negotiated.unwrap_or(format) != self.format)
    }
    fn prepare(&mut self, format: PcmFormat) -> Result<PcmFormat, Box<dyn Error>> {
        let format = self.negotiated.unwrap_or(format);
        if format != self.format {
            if !self.closed {
                self.close_input();
                self.finish()?;
            }
            self.reopened += 1;
            self.format = format;
            self.closed = false;
        }
        Ok(format)
    }
    fn close_input(&mut self) {
        self.closed = true;
    }
    fn drain_progress(&mut self) -> Result<output::DrainProgress, Box<dyn Error>> {
        if self.error_at.is_some_and(|at| self.time.get() >= at) {
            return Err("simulated device failure".into());
        }
        Ok(output::DrainProgress {
            drained: self.time.get() >= self.queued_until,
            consumed_frames: self.native.then(|| {
                if self.progress {
                    self.time.get().min(self.queued_until) / self.progress_interval
                } else {
                    0
                }
            }),
        })
    }
    fn host_tail(&self) -> Duration {
        if self.native {
            Duration::from_millis(100)
        } else {
            Duration::ZERO
        }
    }
    fn finish(&mut self) -> Res {
        self.finalized += 1;
        Ok(())
    }
}

#[test]
fn transport_actions_interrupt_native_host_tail_before_finalization() {
    for action in [
        PlaybackEnd::Stop,
        PlaybackEnd::Next,
        PlaybackEnd::Previous,
        PlaybackEnd::SeekRelative(10_000),
    ] {
        let time = Rc::new(Cell::new(0));
        let mut output = FakeOutput::new(&time);
        let mut clock = PlaybackClock::new();
        let result = drain_output_with(
            &mut output,
            &mut clock,
            |_, _| Ok((time.get() >= 25).then_some(action)),
            |_| time.get(),
            || time.set(time.get() + 25),
        )
        .expect("controlled drain");
        assert_eq!(result, action, "action remains usable during the host tail");
        assert!(output.closed);
        assert_eq!(output.finalized, 0);
    }
}

#[test]
fn transport_actions_cancel_format_reopening_while_queued_output_drains() {
    for native in [false, true] {
        for action in [
            PlaybackEnd::Stop,
            PlaybackEnd::Next,
            PlaybackEnd::Previous,
            PlaybackEnd::SeekRelative(10_000),
        ] {
            let time = Rc::new(Cell::new(0));
            let mut output = FakeOutput::new(&time);
            output.native = native;
            output.queued_until = 200;
            let mut clock = PlaybackClock::new();
            let next = PcmFormat::new(44_100, 2).expect("new format");
            let prepared = prepare_output_with(
                &mut output,
                next,
                &mut clock,
                |_, _| Ok((time.get() >= 25).then_some(action)),
                |_| time.get(),
                || time.set(time.get() + 25),
            )
            .expect("interrupted preparation");
            assert!(matches!(prepared, PreparedOutput::Interrupted(end) if end == action));
            assert_eq!(output.reopened, 0);
            assert_eq!(output.finalized, 0);
        }
    }
}

#[test]
fn a_native_drain_without_callback_progress_returns_an_explicit_error() {
    let time = Rc::new(Cell::new(0));
    let mut output = FakeOutput::new(&time);
    output.queued_until = u64::MAX;
    output.progress = false;
    let mut clock = PlaybackClock::new();
    let result = drain_output_with(
        &mut output,
        &mut clock,
        |_, _| Ok((time.get() >= 6_000).then_some(PlaybackEnd::Stop)),
        |_| time.get(),
        || time.set(time.get() + 25),
    );
    assert!(
        result.is_err(),
        "drain must detect a stalled native consumer before the fallback Stop"
    );
    assert!(
        result
            .expect_err("stalled drain")
            .to_string()
            .contains("stalled")
    );
    assert_eq!(output.finalized, 0);
}

#[test]
fn compatible_negotiated_output_is_reused_without_closing_or_waiting() {
    let time = Rc::new(Cell::new(0));
    let mut output = FakeOutput::new(&time);
    output.queued_until = 500;
    output.negotiated = Some(output.format);
    let held = output.format;
    let mut clock = PlaybackClock::new();
    let source = PcmFormat::new(44_100, 2).expect("different source rate");
    let prepared = prepare_output_with(
        &mut output,
        source,
        &mut clock,
        |_, _| panic!("compatible output must not enter the drain control loop"),
        |_| time.get(),
        || panic!("compatible output must not wait"),
    )
    .expect("reuse selected output format");
    assert!(matches!(prepared, PreparedOutput::Ready(format) if format == held));
    assert!(!output.closed);
    assert_eq!(output.finalized, 0);
    assert_eq!(output.reopened, 0);
}

#[test]
fn incompatible_output_drains_once_before_reopening() {
    for native in [false, true] {
        let time = Rc::new(Cell::new(0));
        let mut output = FakeOutput::new(&time);
        output.native = native;
        output.queued_until = 200;
        let mut clock = PlaybackClock::new();
        let next = PcmFormat::new(44_100, 1).expect("incompatible format");
        let prepared = prepare_output_with(
            &mut output,
            next,
            &mut clock,
            |_, _| Ok(None),
            |_| time.get(),
            || time.set(time.get() + 25),
        )
        .expect("natural format transition");
        assert!(matches!(prepared, PreparedOutput::Ready(format) if format == next));
        assert_eq!(time.get(), if native { 300 } else { 200 });
        assert_eq!(output.finalized, 1);
        assert_eq!(output.reopened, 1);
        assert!(!output.closed);
    }
}

#[test]
fn pauses_during_queued_drain_and_host_tail_do_not_expire_active_time() {
    for pause_at in [100, 225] {
        let time = Rc::new(Cell::new(0));
        let wall = Rc::new(Cell::new(0));
        let mut output = FakeOutput::new(&time);
        output.queued_until = 200;
        let mut clock = PlaybackClock::new();
        let mut paused_once = false;
        let result = drain_output_with(
            &mut output,
            &mut clock,
            |output, clock| {
                if !paused_once && time.get() == pause_at {
                    clock.toggle_pause(output)?;
                    wall.set(wall.get() + 10_000);
                    clock.toggle_pause(output)?;
                    paused_once = true;
                }
                Ok(None)
            },
            |_| time.get(),
            || {
                time.set(time.get() + 25);
                wall.set(wall.get() + 25);
            },
        )
        .expect("paused drain");
        assert_eq!(result, PlaybackEnd::Natural);
        assert!(paused_once);
        assert_eq!(time.get(), 300);
        assert_eq!(wall.get(), 10_300);
        assert_eq!(output.pause_calls.get(), 1);
        assert_eq!(output.resume_calls.get(), 1);
        assert_eq!(output.finalized, 1);
        assert!(clock.paused_since.is_none());
    }
}

#[test]
fn the_playback_clock_excludes_paused_time_from_host_tail_and_stall_deadlines() {
    let anchor = Instant::now();
    let mut clock = PlaybackClock::new();
    clock.started = anchor - Duration::from_secs(30);
    clock.paused_since = Some(clock.started + Duration::from_secs(1));
    assert_eq!(clock.active_ms(), 1_000);
    let progress = output::DrainProgress {
        drained: false,
        consumed_frames: Some(0),
    };
    let mut stalled = DrainWait::new(1_000);
    assert!(
        !stalled
            .advance(clock.active_ms(), progress, Duration::ZERO)
            .unwrap()
    );
    let mut tail = DrainWait::new(1_000);
    let drained = output::DrainProgress {
        drained: true,
        consumed_frames: Some(1),
    };
    let allowance = Duration::from_millis(100);
    assert!(!tail.advance(clock.active_ms(), drained, allowance).unwrap());

    // The pause point lies over 28 seconds in the past. Its wall time must not
    // finish the host allowance or trip the five-second progress watchdog.
    clock.paused_since = Some(clock.started + Duration::from_millis(1_025));
    assert_eq!(clock.active_ms(), 1_025);
    assert!(!tail.advance(clock.active_ms(), drained, allowance).unwrap());
    assert!(
        !stalled
            .advance(clock.active_ms(), progress, Duration::ZERO)
            .unwrap()
    );

    // A subsequent pause includes ten seconds already excluded by resume.
    clock.paused_duration = Duration::from_secs(10);
    clock.paused_since = Some(clock.started + Duration::from_millis(11_125));
    assert_eq!(clock.active_ms(), 1_125);
    assert!(tail.advance(clock.active_ms(), drained, allowance).unwrap());
    assert!(
        !stalled
            .advance(clock.active_ms(), progress, Duration::ZERO)
            .unwrap()
    );
}

#[test]
fn slow_callback_progress_resets_the_native_drain_watchdog() {
    let time = Rc::new(Cell::new(0));
    let mut output = FakeOutput::new(&time);
    output.queued_until = 11_000;
    output.progress_interval = 4_500;
    let mut clock = PlaybackClock::new();
    let end = drain_output_with(
        &mut output,
        &mut clock,
        |_, _| Ok(None),
        |_| time.get(),
        || time.set(time.get() + 25),
    )
    .expect("coarse callbacks continue to consume frames");
    assert_eq!(end, PlaybackEnd::Natural);
    assert_eq!(time.get(), 11_100);
    assert_eq!(output.finalized, 1);
}

#[test]
fn ffplay_without_consumption_counters_is_not_assumed_stalled() {
    let time = Rc::new(Cell::new(0));
    let mut output = FakeOutput::new(&time);
    output.native = false;
    output.progress = false;
    output.queued_until = 6_000;
    let mut clock = PlaybackClock::new();
    let end = drain_output_with(
        &mut output,
        &mut clock,
        |_, _| Ok(None),
        |_| time.get(),
        || time.set(time.get() + 25),
    )
    .expect("child remains alive until its buffered audio finishes");
    assert_eq!(end, PlaybackEnd::Natural);
    assert_eq!(time.get(), 6_000);
    assert_eq!(output.finalized, 1);
}

#[test]
fn device_failure_during_drain_prevents_finalization_and_reopening() {
    let time = Rc::new(Cell::new(0));
    let mut output = FakeOutput::new(&time);
    output.queued_until = 200;
    output.error_at = Some(25);
    let mut clock = PlaybackClock::new();
    let prepared = prepare_output_with(
        &mut output,
        PcmFormat::new(44_100, 1).expect("next format"),
        &mut clock,
        |_, _| Ok(None),
        |_| time.get(),
        || time.set(time.get() + 25),
    );
    assert!(prepared.is_err());
    assert!(
        prepared
            .err()
            .expect("failure")
            .to_string()
            .contains("device failure")
    );
    assert_eq!(output.finalized, 0);
    assert_eq!(output.reopened, 0);
}
