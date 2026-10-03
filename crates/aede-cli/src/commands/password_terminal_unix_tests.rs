use super::*;

#[test]
fn utf8_scalars_preserve_ascii_accents_cjk_and_supplementary_characters() {
    for scalar in [' ', 'a', 'é', '音', '🔑'] {
        let mut encoded = [0; 4];
        let bytes = scalar.encode_utf8(&mut encoded).as_bytes();
        assert_eq!(character(bytes[0], &bytes[1..]).unwrap(), scalar);
    }
}

#[test]
fn corrupt_or_truncated_utf8_never_becomes_a_password_character() {
    for bytes in [
        &[0x80][..],
        &[0xc0, 0xaf],
        &[0xed, 0xa0, 0x80],
        &[0xf4, 0x90, 0x80, 0x80],
        &[0xc3, b'x'],
        &[0xc3],
        &[0xf0, 0x9f, 0x94],
    ] {
        assert!(character(bytes[0], &bytes[1..]).is_err(), "{bytes:?}");
    }
}

fn sequence(bytes: &[u8]) -> io::Result<Key> {
    let mut bytes = bytes.iter().copied();
    escape(|| Ok(bytes.next()))
}

#[test]
fn lone_escape_cancels_while_navigation_and_paste_delimiters_are_ignored() {
    assert_eq!(sequence(b"").unwrap(), Key::Escape);
    for bytes in [
        &b"[A"[..],
        b"[D",
        b"OA",
        b"[1;5C",
        b"[3~",
        b"[200~",
        b"[201~",
        b"x",
        "é".as_bytes(),
    ] {
        assert_eq!(sequence(bytes).unwrap(), Key::Ignored);
    }
}

#[test]
fn incomplete_oversized_or_invalid_key_sequences_are_refused() {
    for bytes in [&b"["[..], b"[123", b"[12345678901234567A", &[0xc3], &[0xff]] {
        assert!(sequence(bytes).is_err(), "{bytes:?}");
    }
}

#[test]
fn cancellation_also_works_during_an_incomplete_navigation_sequence() {
    for prefix in [&b""[..], b"[", b"[1;"] {
        for (byte, expected) in [(3, Key::CtrlC), (4, Key::Char('\u{4}')), (27, Key::Escape)] {
            let mut bytes = prefix.to_vec();
            bytes.push(byte);
            assert_eq!(sequence(&bytes).unwrap(), expected);
        }
    }
}

#[test]
fn pending_input_is_discarded_as_bytes_even_after_an_interrupted_read() {
    struct Pending {
        interrupted: bool,
        bytes: io::Cursor<Vec<u8>>,
    }
    impl Read for Pending {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            self.bytes.read(buffer)
        }
    }
    let bytes = vec![0xff; 3400];
    let mut pending = Pending {
        interrupted: false,
        bytes: io::Cursor::new(bytes),
    };
    discard_pending(&mut pending).unwrap();
    assert_eq!(pending.bytes.position(), 3400);
}

#[test]
fn failed_discard_prevents_successful_cleanup() {
    struct Failed;
    impl Read for Failed {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
    }
    assert_eq!(
        discard_pending(Failed).unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}
