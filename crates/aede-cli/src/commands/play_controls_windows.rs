//! Windows key events mapped to the same transport actions as Unix input.

use std::io;

#[cfg(windows)]
use super::super::super::windows_console::Console;
use super::super::super::windows_console::KeyEvent;
use super::{Action, character_action};

const ALT_KEYS: u32 = 0x0003;
const CTRL_KEYS: u32 = 0x000c;

fn key_action(key: KeyEvent) -> Option<Action> {
    if key.key_down == 0 || key.repeat_count == 0 {
        return None;
    }
    // Windows represents AltGr as right Alt plus left Ctrl. French layouts
    // need it for the brackets used to seek; other shortcuts stay inert.
    if key.control_key_state & (ALT_KEYS | CTRL_KEYS) == 0x0009
        && matches!(key.unicode_char, 0x5b | 0x5d)
    {
        return character_action(key.unicode_char as u8);
    }
    if key.control_key_state & ALT_KEYS != 0 {
        return None;
    }
    if key.control_key_state & CTRL_KEYS != 0 {
        return (key.unicode_char == 3).then_some(Action::Stop);
    }
    match key.virtual_key {
        0x25 => Some(Action::Previous),
        0x27 => Some(Action::Next),
        _ => u8::try_from(key.unicode_char)
            .ok()
            .and_then(character_action),
    }
}

pub(super) trait KeySource {
    fn poll_key(&mut self) -> io::Result<Option<KeyEvent>>;
    fn read_key(&mut self) -> io::Result<KeyEvent>;
}

#[cfg(windows)]
impl KeySource for Console {
    fn poll_key(&mut self) -> io::Result<Option<KeyEvent>> {
        self.poll_key()
    }

    fn read_key(&mut self) -> io::Result<KeyEvent> {
        self.read_key()
    }
}

pub(super) struct Keyboard<S> {
    source: S,
    repeated: Option<(Action, u16)>,
}

impl<S: KeySource> Keyboard<S> {
    fn new(source: S) -> Self {
        Self {
            source,
            repeated: None,
        }
    }

    fn pending(&mut self) -> Option<Action> {
        let (action, remaining) = self.repeated?;
        self.repeated = (remaining > 1).then(|| (action, remaining - 1));
        Some(action)
    }

    fn accept(&mut self, key: KeyEvent) -> Option<Action> {
        let action = key_action(key)?;
        // Expand held-key repeats one action at a time, without allocating
        // a queue whose size depends on the console's repeat count.
        self.repeated = (key.repeat_count > 1).then(|| (action, key.repeat_count - 1));
        Some(action)
    }

    pub(super) fn poll(&mut self) -> io::Result<Option<Action>> {
        if let Some(action) = self.pending() {
            return Ok(Some(action));
        }
        Ok(self.source.poll_key()?.and_then(|key| self.accept(key)))
    }

    pub(super) fn wait(&mut self) -> io::Result<Option<Action>> {
        if let Some(action) = self.pending() {
            return Ok(Some(action));
        }
        loop {
            let key = self.source.read_key()?;
            if let Some(action) = self.accept(key) {
                return Ok(Some(action));
            }
        }
    }
}

#[cfg(windows)]
pub(super) type Input = Keyboard<Console>;

#[cfg(windows)]
impl Input {
    pub(super) fn start() -> io::Result<Self> {
        Ok(Self::new(Console::start_playback()?))
    }
}

#[cfg(test)]
#[path = "play_controls_windows_tests.rs"]
mod tests;
