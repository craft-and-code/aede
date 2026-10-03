//! Windows console input with echo disabled continuously until restoration.

use std::io;

#[cfg(windows)]
use std::{ffi::c_void, os::windows::io::AsRawHandle};

const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
const ENABLE_LINE_INPUT: u32 = 0x0002;
const ENABLE_ECHO_INPUT: u32 = 0x0004;
const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;

fn password_mode(original: u32) -> u32 {
    // Ctrl-C must reach our input reader instead of letting Windows terminate
    // the process before it can restore echo. Disable cooked editing and
    // escape sequences too; the portable collector handles the characters.
    original
        & !(ENABLE_PROCESSED_INPUT
            | ENABLE_LINE_INPUT
            | ENABLE_ECHO_INPUT
            | ENABLE_VIRTUAL_TERMINAL_INPUT)
}

#[derive(Clone, Copy)]
#[repr(C)]
struct KeyEvent {
    key_down: i32,
    repeat_count: u16,
    virtual_key: u16,
    virtual_scan: u16,
    unicode_char: u16,
    control_key_state: u32,
}

#[repr(C)]
union EventData {
    key: KeyEvent,
    // The largest INPUT_RECORD union member is 16 bytes with alignment 4.
    // Non-key records are discarded without interpreting their contents.
    reserved: [u32; 4],
}

#[repr(C)]
struct InputRecord {
    event_type: u16,
    event: EventData,
}

struct KeyUnit {
    unit: u16,
    repeat_count: u16,
}

fn key_unit(record: &InputRecord) -> Option<KeyUnit> {
    if record.event_type != 1 {
        return None;
    }
    // SAFETY: KEY_EVENT identifies this union's KEY_EVENT_RECORD member, whose
    // fields are fixed-size integers and accept every bit pattern. The record
    // is fully initialized by ReadConsoleInputW (or the test fixture).
    let key = unsafe { record.event.key };
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

/// Borrows the process's standard-input console handle without closing it.
#[cfg(windows)]
pub(super) struct Input {
    handle: *mut c_void,
    original: u32,
    restored: bool,
    characters: CharacterInput,
}

#[cfg(windows)]
impl Input {
    pub(super) fn start() -> io::Result<Self> {
        let handle = io::stdin().as_raw_handle();
        let mut original = 0;
        // SAFETY: the standard-input handle is borrowed from Rust and remains
        // open for this guard's lifetime. `original` is a writable DWORD; the
        // API also verifies that the handle refers to a console input buffer.
        if unsafe { GetConsoleMode(handle, &mut original) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the console handle was just validated, and the mode retains
        // all unrelated original flags. SetConsoleMode takes no pointers.
        if unsafe { SetConsoleMode(handle, password_mode(original)) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle,
            original,
            restored: false,
            characters: CharacterInput::default(),
        })
    }

    pub(super) fn read_character(&mut self) -> io::Result<char> {
        let handle = self.handle;
        self.characters.read_character(|| read_key(handle))
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        if self.restored {
            return Ok(());
        }
        self.characters = CharacterInput::default();
        // SAFETY: this is the same borrowed, open console-input handle. Flush
        // discards any queued pasted password suffix before echo can resume.
        // If clearing fails, retain masking and let Drop retry the cleanup.
        if unsafe { FlushConsoleInputBuffer(self.handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the borrowed handle is still open, and `original` is the
        // complete mode returned by GetConsoleMode before this guard changed it.
        if unsafe { SetConsoleMode(self.handle, self.original) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.restored = true;
        Ok(())
    }
}

#[cfg(windows)]
fn read_key(handle: *mut c_void) -> io::Result<KeyUnit> {
    loop {
        let mut record = InputRecord {
            event_type: 0,
            event: EventData { reserved: [0; 4] },
        };
        let mut count = 0u32;
        // SAFETY: the guard borrows a validated open console handle. `record`
        // has the C INPUT_RECORD layout and provides space for one requested
        // event; `count` is a writable DWORD. ReadConsoleInputW is synchronous
        // and does not retain either pointer after returning.
        if unsafe { ReadConsoleInputW(handle, &mut record, 1, &mut count) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if count != 1 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "terminal password input ended unexpectedly",
            ));
        }
        if let Some(key) = key_unit(&record) {
            return Ok(key);
        }
    }
}

#[cfg(windows)]
impl Drop for Input {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("could not restore terminal input mode: {error}");
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: *mut c_void, mode: u32) -> i32;
    fn FlushConsoleInputBuffer(handle: *mut c_void) -> i32;
    fn ReadConsoleInputW(
        handle: *mut c_void,
        buffer: *mut InputRecord,
        length: u32,
        read: *mut u32,
    ) -> i32;
}

#[cfg(test)]
#[path = "password_terminal_windows_tests.rs"]
mod tests;
