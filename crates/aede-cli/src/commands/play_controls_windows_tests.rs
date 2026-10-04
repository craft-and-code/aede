use std::collections::VecDeque;

use super::*;

fn key(virtual_key: u16, character: u16) -> KeyEvent {
    KeyEvent {
        key_down: 1,
        repeat_count: 1,
        virtual_key,
        virtual_scan: 0,
        unicode_char: character,
        control_key_state: 0,
    }
}

#[derive(Default)]
struct Source {
    events: VecDeque<io::Result<KeyEvent>>,
}

impl KeySource for Source {
    fn poll_key(&mut self) -> io::Result<Option<KeyEvent>> {
        self.events.pop_front().transpose()
    }

    fn read_key(&mut self) -> io::Result<KeyEvent> {
        self.events
            .pop_front()
            .unwrap_or_else(|| Err(io::Error::from(io::ErrorKind::UnexpectedEof)))
    }
}

fn keyboard(events: impl IntoIterator<Item = KeyEvent>) -> Keyboard<Source> {
    Keyboard::new(Source {
        events: events.into_iter().map(Ok).collect(),
    })
}

#[test]
fn windows_keys_drive_every_transport_action_without_enter() {
    for (character, expected) in [
        (b' ', Action::Pause),
        (b'n', Action::Next),
        (b'N', Action::Next),
        (b'p', Action::Previous),
        (b'P', Action::Previous),
        (b'[', Action::SeekRelative(-10_000)),
        (b']', Action::SeekRelative(10_000)),
        (b'r', Action::CycleRepeat),
        (b'R', Action::CycleRepeat),
        (b'z', Action::CycleShuffle),
        (b'Z', Action::CycleShuffle),
        (b'q', Action::Stop),
        (b'Q', Action::Stop),
        (b's', Action::Stop),
        (b'S', Action::Stop),
        (3, Action::Stop),
    ] {
        assert_eq!(key_action(key(0, u16::from(character))), Some(expected));
    }
    assert_eq!(key_action(key(0x25, 0)), Some(Action::Previous));
    assert_eq!(key_action(key(0x27, 0)), Some(Action::Next));
    let mut ctrl_c = key(0x43, 3);
    for modifier in [4, 8, 12] {
        ctrl_c.control_key_state = modifier;
        assert_eq!(key_action(ctrl_c), Some(Action::Stop));
    }
}

#[test]
fn windows_french_layout_altgr_brackets_seek_without_enabling_other_shortcuts() {
    for modifiers in [0x0009, 0x0019, 0x0089] {
        for (character, delta) in [(b'[', -10_000), (b']', 10_000)] {
            let mut bracket = key(0, u16::from(character));
            bracket.control_key_state = modifiers;
            assert_eq!(key_action(bracket), Some(Action::SeekRelative(delta)));
        }
        let mut other = key(0, u16::from(b'q'));
        other.control_key_state = modifiers;
        assert_eq!(key_action(other), None);
    }
    for modifiers in [1, 2, 4, 8, 0x0006, 0x000d] {
        let mut bracket = key(0, u16::from(b']'));
        bracket.control_key_state = modifiers;
        assert_eq!(key_action(bracket), None);
    }
}

#[test]
fn windows_releases_shortcuts_and_unrelated_unicode_do_not_control_audio() {
    let mut released = key(0, u16::from(b'q'));
    released.key_down = 0;
    assert_eq!(key_action(released), None);
    let mut no_repeat = key(0x27, 0);
    no_repeat.repeat_count = 0;
    assert_eq!(key_action(no_repeat), None);
    for (virtual_key, character) in [
        (0x26, 0),
        (0x28, 0),
        (0x10, 0),
        (0x11, 0),
        (0x12, 0),
        (0x1b, 0x1b),
        (0, 13),
        (0, 0x00e9),
        (0, 0xd83c),
        (0, 0xdfb5),
    ] {
        assert_eq!(key_action(key(virtual_key, character)), None);
    }
    for modifier in [1, 2, 4, 8, 3, 12] {
        let mut shortcut = key(0x52, u16::from(b'r'));
        shortcut.control_key_state = modifier;
        assert_eq!(key_action(shortcut), None);
        shortcut.virtual_key = 0x27;
        shortcut.unicode_char = 0;
        assert_eq!(key_action(shortcut), None);
    }
    let mut caps_lock = key(0x4e, u16::from(b'N'));
    caps_lock.control_key_state = 0x80;
    assert_eq!(key_action(caps_lock), Some(Action::Next));
}

#[test]
fn windows_held_keys_keep_their_repeat_count_and_event_order() {
    let mut repeated = key(0x27, 0);
    repeated.repeat_count = 3;
    let mut input = keyboard([repeated, key(0, u16::from(b'q'))]);
    assert_eq!(input.poll().unwrap(), Some(Action::Next));
    assert_eq!(input.wait().unwrap(), Some(Action::Next));
    assert_eq!(input.poll().unwrap(), Some(Action::Next));
    assert_eq!(input.poll().unwrap(), Some(Action::Stop));
    assert_eq!(input.poll().unwrap(), None);
}

#[test]
fn windows_wait_ignores_key_releases_and_resumes_on_the_next_command() {
    let mut released = key(0, u16::from(b' '));
    released.key_down = 0;
    let mut input = keyboard([key(0x10, 0), released, key(0, u16::from(b'r'))]);
    assert_eq!(input.wait().unwrap(), Some(Action::CycleRepeat));
    assert_eq!(input.poll().unwrap(), None);
}

#[test]
fn windows_keyboard_errors_reach_the_playback_driver() {
    for blocking in [false, true] {
        let mut input = Keyboard::new(Source {
            events: [Err(io::Error::from(io::ErrorKind::PermissionDenied))].into(),
        });
        let result = if blocking { input.wait() } else { input.poll() };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }
}
