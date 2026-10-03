//! Interactive terminal transport controls for local playback.

use std::error::Error;
use std::sync::mpsc::Receiver;

#[cfg(unix)]
use super::super::terminal_mode::TerminalMode;
#[cfg(unix)]
use std::io::{self, IsTerminal, Read};
#[cfg(unix)]
use std::sync::mpsc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Pause,
    Next,
    Previous,
    Stop,
}

#[cfg(any(unix, test))]
#[derive(Default)]
struct KeyParser {
    escape: u8,
}

#[cfg(any(unix, test))]
impl KeyParser {
    fn feed(&mut self, byte: u8) -> Option<Action> {
        match (self.escape, byte) {
            (0, 0x1b) => {
                self.escape = 1;
                None
            }
            (1, b'[') => {
                self.escape = 2;
                None
            }
            (2, b'C') => {
                self.escape = 0;
                Some(Action::Next)
            }
            (2, b'D') => {
                self.escape = 0;
                Some(Action::Previous)
            }
            (_, byte) => {
                self.escape = 0;
                match byte {
                    b' ' => Some(Action::Pause),
                    b'n' | b'N' => Some(Action::Next),
                    b'p' | b'P' => Some(Action::Previous),
                    b'q' | b'Q' | b's' | b'S' => Some(Action::Stop),
                    _ => None,
                }
            }
        }
    }
}

pub(super) struct Controls {
    actions: Receiver<Action>,
    #[cfg(unix)]
    _terminal_mode: TerminalMode,
}

impl Controls {
    #[cfg(unix)]
    pub(super) fn start() -> Result<Option<Self>, Box<dyn Error>> {
        if !io::stdin().is_terminal() {
            return Ok(None);
        }
        let terminal_mode = TerminalMode::start(&["-icanon", "-echo", "min", "1", "time", "0"])?;
        let (sender, actions) = mpsc::channel();
        std::thread::spawn(move || {
            let mut input = io::stdin().lock();
            let mut parser = KeyParser::default();
            let mut byte = [0];
            loop {
                match input.read(&mut byte) {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Some(action) = parser.feed(byte[0])
                            && sender.send(action).is_err()
                        {
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        eprintln!("playback keyboard controls stopped: {error}");
                        break;
                    }
                }
            }
        });
        Ok(Some(Self {
            actions,
            _terminal_mode: terminal_mode,
        }))
    }

    #[cfg(not(unix))]
    pub(super) fn start() -> Result<Option<Self>, Box<dyn Error>> {
        Ok(None)
    }

    pub(super) fn poll(&self) -> Option<Action> {
        self.actions.try_recv().ok()
    }

    pub(super) fn wait(&self) -> Option<Action> {
        self.actions.recv().ok()
    }
}

#[cfg(test)]
#[path = "play_controls_tests.rs"]
mod tests;
