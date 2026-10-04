//! Windows console input with echo disabled continuously until restoration.

use std::io;

use crate::commands::windows_console::KeyEvent;

#[cfg(windows)]
use crate::commands::windows_console::Console;

struct KeyUnit {
    unit: u16,
    repeat_count: u16,
}

fn key_character(key: KeyEvent) -> Option<KeyUnit> {
    // Windows emits Alt+Numpad's translated character when Alt is released;
    // ordinary key releases must not duplicate characters already entered.
    let alt_character = key.virtual_key == 0x12 && key.unicode_char != 0;
    if (key.key_down == 0 && !alt_character) || key.repeat_count == 0 {
        return None;
    }
    let unit = if key.virtual_key == 0x1b {
        // Unlike ReadConsoleW, key events retain Escape even when it has no
        // Unicode character. Password entry must always be cancellable.
        0x1b
    } else if key.unicode_char != 0 {
        key.unicode_char
    } else {
        return None;
    };
    Some(KeyUnit {
        unit,
        repeat_count: key.repeat_count,
    })
}

#[derive(Default)]
struct CharacterInput {
    repeated: Option<(char, u16)>,
}

impl CharacterInput {
    fn read_character(
        &mut self,
        mut next: impl FnMut() -> io::Result<KeyUnit>,
    ) -> io::Result<char> {
        if let Some((character, remaining)) = self.repeated {
            self.repeated = (remaining > 1).then(|| (character, remaining - 1));
            return Ok(character);
        }
        let first = next()?;
        let mut first_unit = Some(first.unit);
        let character = read_scalar(|| {
            if let Some(unit) = first_unit.take() {
                return Ok(unit);
            }
            let continuation = next()?;
            if continuation.repeat_count != first.repeat_count {
                return Err(invalid_unicode());
            }
            Ok(continuation.unit)
        })?;
        // Repeat complete Unicode scalars, rather than repeating each half of
        // a surrogate pair and turning a held non-BMP character into an error.
        self.repeated = (first.repeat_count > 1).then(|| (character, first.repeat_count - 1));
        Ok(character)
    }
}

fn read_scalar(mut next: impl FnMut() -> io::Result<u16>) -> io::Result<char> {
    let first = next()?;
    let value = if (0xd800..=0xdbff).contains(&first) {
        let second = next()?;
        if !(0xdc00..=0xdfff).contains(&second) {
            return Err(invalid_unicode());
        }
        0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(second) - 0xdc00)
    } else {
        u32::from(first)
    };
    char::from_u32(value).ok_or_else(invalid_unicode)
}

fn invalid_unicode() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "terminal input contains invalid Unicode",
    )
}

/// Applies password-specific Unicode handling to the shared console guard.
#[cfg(windows)]
pub(super) struct Input {
    console: Console,
    characters: CharacterInput,
}

#[cfg(windows)]
impl Input {
    pub(super) fn start() -> io::Result<Self> {
        Ok(Self {
            console: Console::start()?,
            characters: CharacterInput::default(),
        })
    }

    pub(super) fn read_character(&mut self) -> io::Result<char> {
        let console = &mut self.console;
        self.characters.read_character(|| {
            loop {
                if let Some(key) = key_character(console.read_key()?) {
                    return Ok(key);
                }
            }
        })
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        self.characters = CharacterInput::default();
        self.console.restore()
    }
}

#[cfg(test)]
#[path = "password_terminal_windows_tests.rs"]
mod tests;
