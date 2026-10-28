use super::*;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};

struct State {
    position: u64,
    length: u64,
    delivered: u64,
    paused: bool,
    completed: bool,
    error: Option<String>,
    cleanup_error: bool,
    closed: usize,
}
struct Mock(Rc<RefCell<State>>);
const RATE: u32 = 2_822_400;

#[test]
fn dop_stop_and_selection_end_never_authorize_another_native_visit() {
    let paths = [PathBuf::from("original.dsf")];
    for end in [PlaybackEnd::Stop, PlaybackEnd::Next, PlaybackEnd::Natural] {
        let mut order = PlaybackOrder::new(&paths, None, &Default::default()).unwrap();
        assert!(
            !advance_selection(&mut order, &PlaybackClock::new(), None, &paths, end, 1000,)
                .unwrap(),
            "{end:?} must end this selection"
        );
        if end == PlaybackEnd::Stop {
            assert_eq!(
                order.current(),
                Some(0),
                "the presentation cursor is retained"
            );
        }
    }
}

#[test]
fn dop_natural_repeat_and_a_real_successor_still_authorize_playback() {
    let paths = [PathBuf::from("first.dsf"), PathBuf::from("second.dsf")];
    let mut order = PlaybackOrder::new(&paths, None, &Default::default()).unwrap();
    let mut clock = PlaybackClock::new();
    assert!(advance_selection(&mut order, &clock, None, &paths, PlaybackEnd::Next, 0).unwrap());
    assert_eq!(order.current(), Some(1));
    for repeat in [Repeat::One, Repeat::All] {
        clock.repeat = repeat;
        assert!(
            advance_selection(&mut order, &clock, None, &paths, PlaybackEnd::Natural, 1000)
                .unwrap()
        );
        assert_eq!(
            order.current(),
            Some(if repeat == Repeat::One { 1 } else { 0 })
        );
    }
}

#[test]
fn queued_dop_controls_precede_admission_and_keep_pause_across_a_manual_skip() {
    let mut clock = PlaybackClock::new();
    let mut seek = 2000;
    let mut paused = false;
    let mut keys = VecDeque::from([Some(Action::Pause), Some(Action::Next)]);
    let mut waiting = Vec::new();
    let end = before_open(&mut clock, &mut seek, &mut paused, |blocked| {
        waiting.push(blocked);
        Ok(keys.pop_front().flatten())
    })
    .unwrap();
    assert_eq!(end, Some(PlaybackEnd::Next));
    assert_eq!(waiting, [false, true]);
    assert!(paused);

    let mut keys = VecDeque::from([
        Some(Action::CycleRepeat),
        Some(Action::CycleShuffle),
        Some(Action::SeekRelative(-10_000)),
        Some(Action::Pause),
        None,
    ]);
    waiting.clear();
    let end = before_open(&mut clock, &mut seek, &mut paused, |blocked| {
        waiting.push(blocked);
        Ok(keys.pop_front().flatten())
    })
    .unwrap();
    assert_eq!(end, None);
    assert_eq!(waiting, [true, true, true, true, false]);
    assert!(!paused);
    assert_eq!(seek, 0);
    assert_eq!(clock.repeat, Repeat::One);
    assert_eq!(clock.shuffle, crate::args::PlaybackShuffle::Random);
}

#[test]
fn pending_dop_seek_saturates_and_keyboard_errors_refuse_admission() {
    let mut clock = PlaybackClock::new();
    let mut seek = u64::MAX - 100;
    let mut paused = false;
    let mut keys = VecDeque::from([Some(Action::SeekRelative(10_000)), None]);
    assert_eq!(
        before_open(&mut clock, &mut seek, &mut paused, |_| {
            Ok(keys.pop_front().flatten())
        })
        .unwrap(),
        None
    );
    assert_eq!(seek, u64::MAX);
    assert!(
        before_open(&mut clock, &mut seek, &mut paused, |_| {
            Err(std::io::Error::other("keyboard retired"))
        })
        .is_err()
    );
}

#[test]
fn paused_dop_preparation_releases_a_closed_keyboard_without_spinning() {
    let mut waiting = Vec::new();
    let mut paused = true;
    let end = before_open(&mut PlaybackClock::new(), &mut 0, &mut paused, |blocked| {
        waiting.push(blocked);
        Ok(None)
    })
    .unwrap();
    assert_eq!(end, None);
    assert_eq!(waiting, [true, false]);
    assert!(!paused);
}

impl State {
    fn advance(&mut self, samples: u64) {
        if !self.paused && self.error.is_none() {
            let count = samples.min(self.length - self.position);
            self.position += count;
            self.delivered += count;
            self.completed = self.position == self.length;
        }
    }
}
impl Transport for Mock {
    fn snapshot(&self) -> Progress {
        let state = self.0.borrow();
        Progress {
            position: state.position,
            rate: RATE,
            length: state.length,
            paused: state.paused,
            completed: state.completed,
            error: state.error.clone(),
        }
    }
    fn pause(&self) {
        self.0.borrow_mut().paused = true;
    }
    fn resume(&self) {
        self.0.borrow_mut().paused = false;
    }
    fn seek(&mut self, frame: u64) -> Result<Progress, String> {
        self.0.borrow_mut().advance(16); // final in-flight callback on old route
        let previous = self.snapshot();
        let mut state = self.0.borrow_mut();
        state.position = (frame * 16).min(state.length);
        state.completed = state.position == state.length;
        Ok(previous)
    }
    fn close(&mut self) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.advance(16);
        state.closed += 1;
        if state.cleanup_error {
            Err("DAC restoration failed".into())
        } else {
            Ok(())
        }
    }
}
fn mock(position: u64, seconds: u64) -> Mock {
    Mock(Rc::new(RefCell::new(State {
        position,
        length: u64::from(RATE) * seconds,
        delivered: 0,
        paused: false,
        completed: false,
        error: None,
        cleanup_error: false,
        closed: 0,
    })))
}

#[test]
fn natural_dop_history_uses_consumed_original_bits_and_confirmed_close() {
    let mut player = mock(0, 1);
    let state = player.0.clone();
    let (visit, end) = drive(
        &mut player,
        &mut PlaybackClock::new(),
        0,
        || Ok(None),
        || state.borrow_mut().advance(u64::from(RATE) / 10),
    );
    assert_eq!(end.unwrap(), PlaybackEnd::Natural);
    assert_eq!(visit.played_ms, 1000);
    assert_eq!(visit.position_ms, 1000);
    assert!(visit.completed);
    assert_eq!(state.borrow().closed, 1);
}

#[test]
fn initial_and_runtime_seeks_exclude_skipped_bits_and_retain_pause() {
    let mut player = mock(u64::from(RATE) * 2, 10);
    player.0.borrow_mut().advance(16); // callback before the CLI starts polling
    let state = player.0.clone();
    let mut actions = VecDeque::from([
        Some(Action::Pause),
        Some(Action::SeekRelative(1000)),
        None,
        Some(Action::Pause),
        None,
        Some(Action::SeekRelative(-1000)),
        None,
        Some(Action::Stop),
    ]);
    let (visit, end) = drive(
        &mut player,
        &mut PlaybackClock::new(),
        2000,
        || Ok(actions.pop_front().flatten()),
        || state.borrow_mut().advance(u64::from(RATE) / 10),
    );
    assert_eq!(end.unwrap(), PlaybackEnd::Stop);
    assert!(!visit.completed);
    assert_eq!(
        visit.played_ms,
        milliseconds(u128::from(state.borrow().delivered), RATE)
    );
    assert_eq!(
        visit.position_ms,
        milliseconds(u128::from(state.borrow().position), RATE)
    );
    assert_eq!(state.borrow().closed, 1);
}

#[test]
fn next_previous_and_stop_keep_final_callback_counts_without_completion() {
    for (action, expected) in [
        (Action::Next, PlaybackEnd::Next),
        (Action::Previous, PlaybackEnd::Previous),
        (Action::Stop, PlaybackEnd::Stop),
    ] {
        let mut player = mock(0, 10);
        player.0.borrow_mut().advance(u64::from(RATE));
        let (visit, end) = drive(
            &mut player,
            &mut PlaybackClock::new(),
            0,
            || Ok(Some(action)),
            || panic!("transport should have stopped"),
        );
        assert_eq!(end.unwrap(), expected);
        assert_eq!(visit.played_ms, 1000);
        assert!(!visit.completed);
        assert_eq!(player.0.borrow().closed, 1);
    }
}

#[test]
fn paused_route_faults_remain_observable_without_waiting_for_a_keyboard_action() {
    let mut player = mock(0, 1);
    player.pause();
    let state = player.0.clone();
    let (visit, end) = drive(
        &mut player,
        &mut PlaybackClock::new(),
        0,
        || Ok(None),
        || {
            state.borrow_mut().error = Some("route disappeared".into());
        },
    );
    assert!(end.unwrap_err().to_string().contains("route disappeared"));
    assert_eq!(visit.played_ms, 0);
    assert!(!visit.completed);
    assert_eq!(state.borrow().closed, 1);
}

#[test]
fn failed_restoration_cannot_publish_a_complete_dop_listen() {
    let mut player = mock(0, 1);
    player.0.borrow_mut().advance(u64::from(RATE));
    player.0.borrow_mut().cleanup_error = true;
    let (visit, end) = drive(
        &mut player,
        &mut PlaybackClock::new(),
        0,
        || Ok(None),
        || {},
    );
    assert!(end.unwrap_err().to_string().contains("restoration failed"));
    assert!(!visit.completed);
    assert_eq!(visit.played_ms, 1000);
}
