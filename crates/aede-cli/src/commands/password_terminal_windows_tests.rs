use std::io;

use super::{
    CharacterInput, EventData, InputRecord, KeyEvent, KeyUnit, key_unit, password_mode, read_scalar,
};

fn key_record(key_down: bool, virtual_key: u16, unit: u16, repeat_count: u16) -> InputRecord {
    InputRecord {
        event_type: 1,
        event: EventData {
            key: KeyEvent {
                key_down: i32::from(key_down),
                repeat_count,
                virtual_key,
                virtual_scan: 0,
                unicode_char: unit,
                control_key_state: 0,
            },
        },
    }
}

#[test]
fn windows_event_layout_matches_console_input_records() {
    assert_eq!(std::mem::size_of::<KeyEvent>(), 16);
    assert_eq!(std::mem::align_of::<KeyEvent>(), 4);
    assert_eq!(std::mem::offset_of!(KeyEvent, unicode_char), 10);
    assert_eq!(std::mem::offset_of!(KeyEvent, control_key_state), 12);
    assert_eq!(std::mem::size_of::<InputRecord>(), 20);
    assert_eq!(std::mem::align_of::<InputRecord>(), 4);
    assert_eq!(std::mem::offset_of!(InputRecord, event), 4);
}

#[test]
fn windows_escape_cancels_even_without_a_unicode_character() {
    let key = key_unit(&key_record(true, 0x1b, 0, 1)).unwrap();
    assert_eq!(key.unit, 0x1b);
    assert_eq!(key.repeat_count, 1);
    assert!(key_unit(&key_record(false, 0x1b, 0, 1)).is_none());
}

#[test]
fn windows_input_ignores_releases_modifiers_navigation_and_non_key_events() {
    assert!(key_unit(&key_record(false, 0x41, 0x61, 1)).is_none());
    for virtual_key in [0x10, 0x11, 0x12, 0x25, 0x26, 0x27, 0x28] {
        assert!(key_unit(&key_record(true, virtual_key, 0, 1)).is_none());
    }
    assert!(key_unit(&key_record(true, 0x41, 0x61, 0)).is_none());
    for event_type in [2, 4, 8, 16] {
        let record = InputRecord {
            event_type,
            event: EventData { reserved: [0; 4] },
        };
        assert!(key_unit(&record).is_none());
    }
    for unit in [3, 4, 8, 13, 21] {
        assert_eq!(key_unit(&key_record(true, 0, unit, 1)).unwrap().unit, unit);
    }
}

#[test]
fn windows_input_accepts_alt_numpad_unicode_on_alt_release_once() {
    assert!(key_unit(&key_record(false, 0x12, 0, 1)).is_none());
    let key = key_unit(&key_record(false, 0x12, 'é' as u16, 1)).unwrap();
    assert_eq!(key.unit, 'é' as u16);
    assert_eq!(key.repeat_count, 1);
    assert!(key_unit(&key_record(false, 0x41, 'é' as u16, 1)).is_none());
}

#[test]
fn windows_repeated_characters_preserve_complete_unicode_scalars() {
    let mut input = CharacterInput::default();
    let mut events = [
        KeyUnit {
            unit: 0x61,
            repeat_count: 2,
        },
        KeyUnit {
            unit: 0xd83c,
            repeat_count: 3,
        },
        KeyUnit {
            unit: 0xdfb5,
            repeat_count: 3,
        },
        KeyUnit {
            unit: 8,
            repeat_count: 2,
        },
    ]
    .into_iter();
    let mut next = || {
        events
            .next()
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))
    };
    for expected in ['a', 'a', '🎵', '🎵', '🎵', '\u{8}', '\u{8}'] {
        assert_eq!(input.read_character(&mut next).unwrap(), expected);
    }
    assert_eq!(
        input.read_character(next).unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn windows_input_refuses_inconsistent_surrogate_repetitions() {
    let mut input = CharacterInput::default();
    let mut events = [
        KeyUnit {
            unit: 0xd83c,
            repeat_count: 2,
        },
        KeyUnit {
            unit: 0xdfb5,
            repeat_count: 1,
        },
    ]
    .into_iter();
    let error = input
        .read_character(|| {
            events
                .next()
                .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))
        })
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert_eq!(error.to_string(), "terminal input contains invalid Unicode");
}

fn characters(units: &[u16]) -> io::Result<char> {
    let mut units = units.iter().copied();
    read_scalar(|| {
        units
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "test input ended"))
    })
}

#[test]
fn password_input_disables_echo_editing_signals_and_terminal_sequences() {
    let original = 0xffff_ffff;
    let masked = password_mode(original);
    assert_eq!(masked & 0x0207, 0);
    assert_eq!(masked | 0x0207, original);
    assert_eq!(password_mode(0), 0);
}

#[test]
fn windows_input_preserves_unicode_and_control_characters() {
    for character in ['a', '\u{0}', '\u{3}', '\r', '\u{8}', 'é', '中', '🎵', '𐀀'] {
        let mut buffer = [0; 2];
        assert_eq!(
            characters(character.encode_utf16(&mut buffer)).unwrap(),
            character
        );
    }
}

#[test]
fn windows_input_consumes_exactly_one_unicode_scalar() {
    let mut units = [0xd83c, 0xdfb5, b'x' as u16].into_iter();
    let mut next = || {
        units
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "test input ended"))
    };
    assert_eq!(read_scalar(&mut next).unwrap(), '🎵');
    assert_eq!(read_scalar(&mut next).unwrap(), 'x');
}

#[test]
fn windows_input_refuses_unpaired_or_truncated_surrogates() {
    for units in [&[0xdc00][..], &[0xd800, 0x0061], &[0xd800, 0xd800]] {
        let error = characters(units).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "terminal input contains invalid Unicode");
    }
    for units in [&[][..], &[0xd800]] {
        assert_eq!(
            characters(units).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}

#[test]
fn windows_input_preserves_read_errors_without_including_secret_input() {
    let error = read_scalar(|| Err(io::Error::from(io::ErrorKind::Interrupted))).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    let mut first = true;
    let error = read_scalar(|| {
        if first {
            first = false;
            Ok(0xd800)
        } else {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}
