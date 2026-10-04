//! Interactive terminal transport controls for local playback.

use std::error::Error;
use std::io;
#[cfg(not(windows))]
use std::sync::mpsc::Receiver;

#[cfg(unix)]
use super::super::terminal_mode::TerminalMode;
#[cfg(any(unix, windows))]
use std::io::IsTerminal;
#[cfg(unix)]
use std::io::Read;
#[cfg(unix)]
use std::sync::mpsc;

#[cfg(any(windows, test))]
#[path = "play_controls_windows.rs"]
mod windows;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Pause,
    Next,
    Previous,
    Stop,
    SeekRelative(i64),
    CycleRepeat,
    CycleShuffle,
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
            (_, 0x1b) => {
                self.escape = 1;
                None
            }
            (1, b'[' | b'O') => {
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
            (2, 0x20..=0x3f) => None,
            (1 | 2, _) => {
                // Consume unknown escape sequences rather than interpreting
                // their final letter as a standalone transport command.
                self.escape = 0;
                None
            }
            (_, byte) => {
                self.escape = 0;
                character_action(byte)
            }
        }
    }
}

fn character_action(byte: u8) -> Option<Action> {
    match byte {
        b' ' => Some(Action::Pause),
        b'n' | b'N' => Some(Action::Next),
        b'p' | b'P' => Some(Action::Previous),
        0x03 | b'q' | b'Q' | b's' | b'S' => Some(Action::Stop),
        b'[' => Some(Action::SeekRelative(-10_000)),
        b']' => Some(Action::SeekRelative(10_000)),
        b'r' | b'R' => Some(Action::CycleRepeat),
        b'z' | b'Z' => Some(Action::CycleShuffle),
        _ => None,
    }
}

pub(super) struct Controls {
    #[cfg(not(windows))]
    actions: Receiver<Action>,
    #[cfg(unix)]
    _terminal_mode: TerminalMode,
    #[cfg(windows)]
    input: std::cell::RefCell<windows::Input>,
}

impl Controls {
    #[cfg(unix)]
    pub(super) fn start() -> Result<Option<Self>, Box<dyn Error>> {
        if !io::stdin().is_terminal() {
            return Ok(None);
        }
        // Read Ctrl-C as an orderly Stop instead of terminating before the
        // guard can restore input mode and the driver can finish history.
        let terminal_mode =
            TerminalMode::start(&["-icanon", "-echo", "-isig", "min", "1", "time", "0"])?;
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

    #[cfg(windows)]
    pub(super) fn start() -> Result<Option<Self>, Box<dyn Error>> {
        if !io::stdin().is_terminal() {
            return Ok(None);
        }
        Ok(Some(Self {
            input: std::cell::RefCell::new(windows::Input::start()?),
        }))
    }

    #[cfg(not(any(unix, windows)))]
    pub(super) fn start() -> Result<Option<Self>, Box<dyn Error>> {
        Ok(None)
    }

    pub(super) fn poll(&self) -> io::Result<Option<Action>> {
        #[cfg(not(windows))]
        return Ok(self.actions.try_recv().ok());
        #[cfg(windows)]
        self.input.borrow_mut().poll()
    }

    pub(super) fn wait(&self) -> io::Result<Option<Action>> {
        #[cfg(not(windows))]
        return Ok(self.actions.recv().ok());
        #[cfg(windows)]
        self.input.borrow_mut().wait()
    }
}

#[cfg(test)]
#[path = "play_controls_tests.rs"]
mod tests;
