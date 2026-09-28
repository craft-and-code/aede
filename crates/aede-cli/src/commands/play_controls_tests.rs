use super::{Action, KeyParser};

#[test]
fn one_key_controls_playback_without_enter() {
    let mut parser = KeyParser::default();
    assert_eq!(parser.feed(b' '), Some(Action::Pause));
    assert_eq!(parser.feed(b'n'), Some(Action::Next));
    assert_eq!(parser.feed(b'p'), Some(Action::Previous));
    assert_eq!(parser.feed(b'q'), Some(Action::Stop));
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
}
