use std::{cell::RefCell, collections::VecDeque, io, rc::Rc};

use super::{
    ConsoleMode, EventData, InputRecord, KeyEvent, MAX_POLL_RECORDS, ModeBackend, playback_mode,
    poll_key_with, raw_mode, read_key_with,
};

#[derive(Debug, PartialEq, Eq)]
enum Call {
    ReadMode,
    SetMode(u32),
    Flush,
}

struct State {
    mode: u32,
    calls: Vec<Call>,
    fail_read: bool,
    fail_set: VecDeque<usize>,
    set_count: usize,
    fail_flush: bool,
}

#[derive(Clone)]
struct FakeBackend(Rc<RefCell<State>>);

impl ModeBackend for FakeBackend {
    fn mode(&mut self) -> io::Result<u32> {
        let mut state = self.0.borrow_mut();
        state.calls.push(Call::ReadMode);
        if state.fail_read {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(state.mode)
    }

    fn set_mode(&mut self, mode: u32) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls.push(Call::SetMode(mode));
        state.set_count += 1;
        // A failed activation may still have changed some console state.
        state.mode = mode;
        if state.fail_set.front().copied() == Some(state.set_count) {
            state.fail_set.pop_front();
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self.0.borrow_mut();
        state.calls.push(Call::Flush);
        if state.fail_flush {
            state.fail_flush = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(())
    }
}

fn backend() -> (FakeBackend, Rc<RefCell<State>>) {
    let state = Rc::new(RefCell::new(State {
        mode: 0xffff_ffff,
        calls: Vec::new(),
        fail_read: false,
        fail_set: VecDeque::new(),
        set_count: 0,
        fail_flush: false,
    }));
    (FakeBackend(Rc::clone(&state)), state)
}

#[test]
fn console_restores_the_original_mode_once_after_explicit_cleanup() {
    let (backend, state) = backend();
    let original = state.borrow().mode;
    let mut guard = ConsoleMode::start(backend).unwrap();
    assert_eq!(state.borrow().mode, raw_mode(original));
    guard.restore().unwrap();
    guard.restore().unwrap();
    drop(guard);
    assert_eq!(state.borrow().mode, original);
    assert_eq!(
        state.borrow().calls,
        [
            Call::ReadMode,
            Call::SetMode(raw_mode(original)),
            Call::Flush,
            Call::SetMode(original),
        ]
    );
}

#[test]
fn playback_disables_selection_freezes_and_restores_the_original_mode() {
    let (backend, state) = backend();
    // QuickEdit starts enabled and its required extended flag starts disabled.
    let original = 0xffff_ff7f;
    state.borrow_mut().mode = original;
    let guard = ConsoleMode::start_with(backend, playback_mode).unwrap();
    let active = state.borrow().mode;
    assert_eq!(active & 0x0247, 0);
    assert_eq!(active & 0x0080, 0x0080);
    assert_eq!(active & !0x02c7, original & !0x02c7);
    drop(guard);
    assert_eq!(state.borrow().mode, original);
}

#[test]
fn console_refuses_an_unreadable_mode_without_changing_input() {
    let (backend, state) = backend();
    state.borrow_mut().fail_read = true;
    let error = match ConsoleMode::start(backend) {
        Ok(_) => panic!("an unreadable mode must be refused"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(state.borrow().calls, [Call::ReadMode]);
    assert_eq!(state.borrow().mode, 0xffff_ffff);
}

#[test]
fn console_rolls_back_a_partially_failed_activation() {
    let (backend, state) = backend();
    state.borrow_mut().fail_set.push_back(1);
    let error = match ConsoleMode::start(backend) {
        Ok(_) => panic!("failed activation must be refused"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(state.borrow().mode, 0xffff_ffff);
    assert_eq!(
        state.borrow().calls,
        [
            Call::ReadMode,
            Call::SetMode(raw_mode(0xffff_ffff)),
            Call::Flush,
            Call::SetMode(0xffff_ffff),
        ]
    );
}

#[test]
fn console_keeps_input_masked_until_queued_input_can_be_cleared() {
    let (backend, state) = backend();
    let mut guard = ConsoleMode::start(backend).unwrap();
    state.borrow_mut().fail_flush = true;
    assert_eq!(
        guard.restore().unwrap_err().kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(state.borrow().mode, raw_mode(0xffff_ffff));
    assert_eq!(state.borrow().calls.last(), Some(&Call::Flush));
    drop(guard);
    assert_eq!(state.borrow().mode, 0xffff_ffff);
    assert_eq!(state.borrow().set_count, 2);
}

#[test]
fn console_retries_failed_restoration_when_the_guard_is_dropped() {
    let (backend, state) = backend();
    let mut guard = ConsoleMode::start(backend).unwrap();
    state.borrow_mut().fail_set.push_back(2);
    assert_eq!(
        guard.restore().unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    drop(guard);
    assert_eq!(state.borrow().mode, 0xffff_ffff);
    assert_eq!(state.borrow().set_count, 3);
    assert_eq!(
        state
            .borrow()
            .calls
            .iter()
            .filter(|call| **call == Call::Flush)
            .count(),
        2
    );
}

fn other_record() -> InputRecord {
    InputRecord {
        event_type: 2,
        event: EventData { reserved: [0; 4] },
    }
}

fn keyboard_record(key_down: bool) -> InputRecord {
    InputRecord {
        event_type: 1,
        event: EventData {
            key: KeyEvent {
                key_down: i32::from(key_down),
                repeat_count: 3,
                virtual_key: 0x27,
                virtual_scan: 0,
                unicode_char: 0,
                control_key_state: 0x10,
            },
        },
    }
}

#[test]
fn console_poll_bounds_background_events_and_keeps_the_next_key() {
    let mut events: VecDeque<_> = (0..MAX_POLL_RECORDS).map(|_| other_record()).collect();
    events.push_back(keyboard_record(true));
    let mut consumed = 0;
    assert!(
        poll_key_with(|| {
            consumed += 1;
            Ok(events.pop_front())
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(consumed, MAX_POLL_RECORDS);
    assert_eq!(events.len(), 1);
    let key = poll_key_with(|| Ok(events.pop_front())).unwrap().unwrap();
    assert_eq!(key.virtual_key, 0x27);
    assert_eq!(key.repeat_count, 3);
    assert_eq!(key.control_key_state, 0x10);
    assert!(poll_key_with(|| Ok(events.pop_front())).unwrap().is_none());
}

#[test]
fn console_key_reader_preserves_key_releases_and_reports_read_failures() {
    let mut events = [other_record(), keyboard_record(false)].into_iter();
    let key = read_key_with(|| {
        events
            .next()
            .ok_or_else(|| io::ErrorKind::UnexpectedEof.into())
    })
    .unwrap();
    assert_eq!(key.key_down, 0);
    assert_eq!(key.repeat_count, 3);
    assert_eq!(key.virtual_key, 0x27);
    let error = match read_key_with(|| Err(io::ErrorKind::BrokenPipe.into())) {
        Ok(_) => panic!("input failure must be reported"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn console_poll_reports_pending_input_failures() {
    let error = match poll_key_with(|| Err(io::ErrorKind::PermissionDenied.into())) {
        Ok(_) => panic!("pending input failure must be reported"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}
