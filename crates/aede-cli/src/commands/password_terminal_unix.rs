//! UTF-8 terminal keys using the existing scoped stty mode, without FFI.

use std::io::{self, Read, StdinLock};

use crate::commands::terminal_mode::TerminalMode;

use super::{Key, character_key};

pub(super) struct Input {
    mode: TerminalMode,
    reader: StdinLock<'static>,
    restored: bool,
}

impl Input {
    pub(super) fn start() -> io::Result<Self> {
        let mode = TerminalMode::start(&[
            // Read characters ourselves: neither flow control, extended
            // editing nor input translation may swallow or rewrite a secret.
            "-echo", "-echonl", "-icanon", "-isig", "-iexten", "-ixon", "-ixoff", "-istrip",
            "-inlcr", "-igncr", "-icrnl", "-parmrk", "min", "1", "time", "0",
        ])?;
        Ok(Self {
            mode,
            reader: io::stdin().lock(),
            restored: false,
        })
    }

    pub(super) fn read_key(&mut self) -> io::Result<Key> {
        let mut first = [0];
        self.reader.read_exact(&mut first)?;
        if first[0] == 0x1b {
            // A lone Escape must cancel rather than wait for another key.
            // The nested mode changes only the read timeout, retaining the
            // outer guard's continuous echo/signal suppression.
            let mut timeout = TerminalMode::start(&["min", "0", "time", "1"])?;
            let result = escape(|| {
                let mut byte = [0];
                match self.reader.read(&mut byte)? {
                    0 => Ok(None),
                    _ => Ok(Some(byte[0])),
                }
            });
            timeout.restore()?;
            return result;
        }
        character(first[0], &mut self.reader).map(character_key)
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        // On cancellation or invalid input, the rest of a paste may still be
        // buffered by StdinLock or queued in the terminal driver. Discard both
        // while echo remains disabled, including extra lines after success.
        // VTIME waits for a quiet 100 ms without waiting indefinitely for a key.
        let mut timeout = TerminalMode::start(&["min", "0", "time", "1"])?;
        discard_pending(&mut self.reader)?;
        timeout.restore()?;
        self.mode.restore()?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        // Cover prompt write/flush failures and unwinding, as well as retrying
        // a failed explicit cleanup. The terminal mode guard is the last resort.
        if let Err(error) = self.restore() {
            // A generic mode-guard drop must not re-enable echo over input
            // that could not be drained. Prefer a masked terminal and a clear
            // error to exposing queued secret bytes; playback keeps its guard.
            self.mode.retain_current();
            eprintln!(
                "cannot finish masked terminal input; terminal echo remains disabled: {error}"
            );
        }
    }
}

fn discard_pending(mut reader: impl Read) -> io::Result<()> {
    let mut discarded = [0; 1024];
    loop {
        match reader.read(&mut discarded) {
            Ok(0) => return Ok(()),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

fn character(first: u8, mut remaining: impl Read) -> io::Result<char> {
    let length = match first {
        0..=0x7f => return Ok(char::from(first)),
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return Err(invalid_utf8()),
    };
    let mut bytes = [0; 4];
    bytes[0] = first;
    remaining.read_exact(&mut bytes[1..length])?;
    std::str::from_utf8(&bytes[..length])
        .map_err(|_| invalid_utf8())?
        .chars()
        .next()
        .ok_or_else(invalid_utf8)
}

fn invalid_utf8() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "terminal input is not valid UTF-8",
    )
}

fn escape(mut next: impl FnMut() -> io::Result<Option<u8>>) -> io::Result<Key> {
    match next()? {
        None => Ok(Key::Escape),
        Some(0x03) => Ok(Key::CtrlC),
        Some(0x04) => Ok(Key::Char('\u{4}')),
        Some(0x1b) => Ok(Key::Escape),
        Some(b'[' | b'O') => {
            // CSI/SS3 keys and bracketed-paste delimiters are terminal events,
            // never password text. Bound both their size and inter-byte wait.
            for _ in 0..16 {
                match next()? {
                    Some(0x03) => return Ok(Key::CtrlC),
                    Some(0x04) => return Ok(Key::Char('\u{4}')),
                    Some(0x1b) => return Ok(Key::Escape),
                    Some(0x40..=0x7e) => return Ok(Key::Ignored),
                    Some(0x20..=0x3f) => {}
                    _ => break,
                }
            }
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "incomplete or oversized terminal key sequence",
            ))
        }
        Some(first) => {
            // Alt plus a printable character is also a key event. Consume
            // its complete scalar so UTF-8 continuation bytes cannot leak
            // into the next password character.
            let mut bytes = [0; 3];
            let length = match first {
                0..=0x7f => 0,
                0xc2..=0xdf => 1,
                0xe0..=0xef => 2,
                0xf0..=0xf4 => 3,
                _ => return Err(invalid_utf8()),
            };
            for byte in &mut bytes[..length] {
                *byte = next()?.ok_or_else(invalid_utf8)?;
            }
            let scalar = character(first, &bytes[..length])?;
            if scalar.is_control() {
                return Ok(character_key(scalar));
            }
            Ok(Key::Ignored)
        }
    }
}

#[cfg(test)]
#[path = "password_terminal_unix_tests.rs"]
mod tests;
