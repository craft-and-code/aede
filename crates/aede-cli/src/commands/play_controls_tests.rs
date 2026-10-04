use super::{Action, KeyParser};

#[test]
fn one_key_controls_playback_without_enter() {
    let mut parser = KeyParser::default();
    assert_eq!(parser.feed(b' '), Some(Action::Pause));
    assert_eq!(parser.feed(b'n'), Some(Action::Next));
    assert_eq!(parser.feed(b'p'), Some(Action::Previous));
    assert_eq!(parser.feed(b'q'), Some(Action::Stop));
    assert_eq!(parser.feed(0x03), Some(Action::Stop));
    assert_eq!(parser.feed(b'['), Some(Action::SeekRelative(-10_000)));
    assert_eq!(parser.feed(b']'), Some(Action::SeekRelative(10_000)));
    assert_eq!(parser.feed(b'r'), Some(Action::CycleRepeat));
    assert_eq!(parser.feed(b'z'), Some(Action::CycleShuffle));
    assert_eq!(parser.feed(b'x'), None);
}

#[test]
fn arrow_keys_select_adjacent_tracks() {
    let mut parser = KeyParser::default();
    assert_eq!(parser.feed(0x1b), None);
    assert_eq!(parser.feed(b'['), None);
    assert_eq!(parser.feed(b'C'), Some(Action::Next));
    assert_eq!(parser.feed(0x1b), None);
    assert_eq!(parser.feed(b'['), None);
    assert_eq!(parser.feed(b'D'), Some(Action::Previous));
    for byte in [0x1b, 0x1b, b'['] {
        assert_eq!(parser.feed(byte), None);
    }
    assert_eq!(parser.feed(b'D'), Some(Action::Previous));
}

#[test]
fn escape_sequences_do_not_trigger_plain_transport_keys() {
    let mut parser = KeyParser::default();
    for byte in [0x1b, b'[', b'1', b';', b'2', b'r'] {
        assert_eq!(parser.feed(byte), None);
    }
    assert_eq!(parser.feed(b'r'), Some(Action::CycleRepeat));
    for byte in [0x1b, b'[', b'1', b';', b'5'] {
        assert_eq!(parser.feed(byte), None);
    }
    assert_eq!(parser.feed(b'C'), Some(Action::Next));
    for byte in [0x1b, b'O'] {
        assert_eq!(parser.feed(byte), None);
    }
    assert_eq!(parser.feed(b'D'), Some(Action::Previous));
}
