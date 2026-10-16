use super::*;

#[test]
fn the_message_names_what_wanted_it_and_how_to_get_it() {
    // "ffmpeg was not found" alone leaves the reader wondering what they
    // have lost and what to do about it.
    let text = missing("--compress");
    assert!(text.starts_with("--compress needs ffmpeg"), "{text}");
    assert!(text.contains("brew install ffmpeg"), "{text}");
    assert!(missing("spectrum").starts_with("spectrum needs ffmpeg"));
}

#[test]
fn diagnostic_capture_drains_every_byte_and_retains_only_the_bounded_tail() {
    for size in [0, 127, 4096, 65_537] {
        let bytes: Vec<_> = (0..size).map(|index| (index % 251) as u8).collect();
        let mut reader = std::io::Cursor::new(&bytes);
        let tail = diagnostic_tail(&mut reader).unwrap();
        assert_eq!(reader.position(), size as u64);
        assert_eq!(tail, bytes[bytes.len().saturating_sub(4096)..]);
    }
}

#[test]
fn diagnostic_capture_resumes_after_an_interrupted_read() {
    use std::io::{Cursor, ErrorKind, Read};

    struct InterruptedOnce {
        bytes: Cursor<Vec<u8>>,
        interrupted: bool,
    }
    impl Read for InterruptedOnce {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if self.bytes.position() >= 1024 && !self.interrupted {
                self.interrupted = true;
                return Err(ErrorKind::Interrupted.into());
            }
            self.bytes.read(output)
        }
    }

    let bytes: Vec<_> = (0..10_000).map(|index| (index % 251) as u8).collect();
    let mut reader = InterruptedOnce {
        bytes: Cursor::new(bytes.clone()),
        interrupted: false,
    };
    let tail = diagnostic_tail(&mut reader).unwrap();
    assert!(reader.interrupted);
    assert_eq!(reader.bytes.position(), bytes.len() as u64);
    assert_eq!(tail, bytes[bytes.len() - 4096..]);
}
