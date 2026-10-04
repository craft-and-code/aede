//! Scoped Windows console input shared by playback and masked password entry.
//!
//! The console input buffer has one consumer while this guard is active. It
//! borrows standard input and restores its complete original mode on exit.

use std::io;

#[cfg(windows)]
use std::{ffi::c_void, os::windows::io::AsRawHandle};

const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
const ENABLE_LINE_INPUT: u32 = 0x0002;
const ENABLE_ECHO_INPUT: u32 = 0x0004;
const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;
const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;
const MAX_POLL_RECORDS: usize = 32;

pub(super) fn raw_mode(original: u32) -> u32 {
    // Ctrl-C must reach the input reader so the command can stop cleanly and
    // restore echo. Key events provide navigation without terminal sequences.
    original
        & !(ENABLE_PROCESSED_INPUT
            | ENABLE_LINE_INPUT
            | ENABLE_ECHO_INPUT
            | ENABLE_VIRTUAL_TERMINAL_INPUT)
}

fn playback_mode(original: u32) -> u32 {
    // Classic console selection can suspend I/O and starve the audio queue.
    // Windows requires EXTENDED_FLAGS when changing QuickEdit's enabled state.
    (raw_mode(original) | ENABLE_EXTENDED_FLAGS) & !ENABLE_QUICK_EDIT_MODE
}

#[derive(Clone, Copy)]
#[repr(C)]
pub(super) struct KeyEvent {
    pub(super) key_down: i32,
    pub(super) repeat_count: u16,
    pub(super) virtual_key: u16,
    pub(super) virtual_scan: u16,
    pub(super) unicode_char: u16,
    pub(super) control_key_state: u32,
}

#[repr(C)]
pub(super) union EventData {
    pub(super) key: KeyEvent,
    // The largest INPUT_RECORD union member is 16 bytes with alignment 4.
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
pub(super) struct InputRecord {
    pub(super) event_type: u16,
    pub(super) event: EventData,
}

pub(super) fn key_event(record: &InputRecord) -> Option<KeyEvent> {
    if record.event_type != 1 {
        return None;
    }
    // SAFETY: KEY_EVENT identifies the KEY_EVENT_RECORD member. Its integer
    // fields accept every bit pattern, and ReadConsoleInputW or a test fixture
    // initializes the complete record before it reaches this function.
    Some(unsafe { record.event.key })
}

fn read_key_with(mut next: impl FnMut() -> io::Result<InputRecord>) -> io::Result<KeyEvent> {
    loop {
        if let Some(key) = key_event(&next()?) {
            return Ok(key);
        }
    }
}

fn poll_key_with(
    mut next: impl FnMut() -> io::Result<Option<InputRecord>>,
) -> io::Result<Option<KeyEvent>> {
    // Mouse, focus and resize traffic must not keep the playback producer
    // away from feeding audio indefinitely. Resume discarding on its next poll.
    for _ in 0..MAX_POLL_RECORDS {
        let Some(record) = next()? else {
            return Ok(None);
        };
        if let Some(key) = key_event(&record) {
            return Ok(Some(key));
        }
    }
    Ok(None)
}

trait ModeBackend {
    fn mode(&mut self) -> io::Result<u32>;
    fn set_mode(&mut self, mode: u32) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}

struct ConsoleMode<B: ModeBackend> {
    backend: B,
    original: Option<u32>,
}

impl<B: ModeBackend> ConsoleMode<B> {
    fn start(backend: B) -> io::Result<Self> {
        Self::start_with(backend, raw_mode)
    }

    fn start_with(mut backend: B, input_mode: fn(u32) -> u32) -> io::Result<Self> {
        let original = backend.mode()?;
        let mut guard = Self {
            backend,
            original: Some(original),
        };
        if let Err(error) = guard.backend.set_mode(input_mode(original)) {
            // Keep the guard armed if activation partially changes the mode.
            guard.restore()?;
            return Err(error);
        }
        Ok(guard)
    }

    fn restore(&mut self) -> io::Result<()> {
        let Some(original) = self.original else {
            return Ok(());
        };
        // In password entry this removes any pasted secret suffix before echo
        // resumes. If clearing fails, retain masking and let Drop retry.
        self.backend.flush()?;
        self.backend.set_mode(original)?;
        self.original = None;
        Ok(())
    }
}

impl<B: ModeBackend> Drop for ConsoleMode<B> {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("could not restore terminal input mode: {error}");
        }
    }
}

/// Borrows the process's standard-input console handle without closing it.
#[cfg(windows)]
pub(super) struct Console {
    mode: ConsoleMode<NativeBackend>,
}

#[cfg(windows)]
impl Console {
    pub(super) fn start() -> io::Result<Self> {
        Ok(Self {
            mode: ConsoleMode::start(NativeBackend {
                handle: io::stdin().as_raw_handle(),
            })?,
        })
    }

    pub(super) fn start_playback() -> io::Result<Self> {
        Ok(Self {
            mode: ConsoleMode::start_with(
                NativeBackend {
                    handle: io::stdin().as_raw_handle(),
                },
                playback_mode,
            )?,
        })
    }

    pub(super) fn read_key(&mut self) -> io::Result<KeyEvent> {
        read_key_with(|| self.mode.backend.read_record())
    }

    pub(super) fn poll_key(&mut self) -> io::Result<Option<KeyEvent>> {
        poll_key_with(|| {
            if self.mode.backend.pending_records()? == 0 {
                return Ok(None);
            }
            self.mode.backend.read_record().map(Some)
        })
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        self.mode.restore()
    }
}

#[cfg(windows)]
struct NativeBackend {
    handle: *mut c_void,
}

#[cfg(windows)]
impl ModeBackend for NativeBackend {
    fn mode(&mut self) -> io::Result<u32> {
        let mut mode = 0;
        // SAFETY: Rust lends standard input's open handle. `mode` is a writable
        // DWORD, and Windows checks that the handle is a console input buffer.
        if unsafe { GetConsoleMode(self.handle, &mut mode) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(mode)
    }

    fn set_mode(&mut self, mode: u32) -> io::Result<()> {
        // SAFETY: mode() validated this borrowed console handle; the complete
        // original mode or selected input mode preserves unrelated flags.
        if unsafe { SetConsoleMode(self.handle, mode) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        // SAFETY: this is the same borrowed, open console-input handle.
        if unsafe { FlushConsoleInputBuffer(self.handle) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
impl NativeBackend {
    fn pending_records(&mut self) -> io::Result<u32> {
        let mut count = 0;
        // SAFETY: the guard owns console-mode access to the validated borrowed
        // handle; `count` is a writable DWORD. This query does not consume input.
        if unsafe { GetNumberOfConsoleInputEvents(self.handle, &mut count) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(count)
    }

    fn read_record(&mut self) -> io::Result<InputRecord> {
        let mut record = InputRecord {
            event_type: 0,
            event: EventData { reserved: [0; 4] },
        };
        let mut count = 0;
        // SAFETY: the borrowed handle remains open, `record` has INPUT_RECORD's
        // C layout and space for one event, and `count` is a writable DWORD.
        // The synchronous API retains neither pointer after it returns.
        if unsafe { ReadConsoleInputW(self.handle, &mut record, 1, &mut count) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if count != 1 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "terminal input ended unexpectedly",
            ));
        }
        Ok(record)
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: *mut c_void, mode: u32) -> i32;
    fn FlushConsoleInputBuffer(handle: *mut c_void) -> i32;
    fn GetNumberOfConsoleInputEvents(handle: *mut c_void, events: *mut u32) -> i32;
    fn ReadConsoleInputW(
        handle: *mut c_void,
        buffer: *mut InputRecord,
        length: u32,
        read: *mut u32,
    ) -> i32;
}

#[cfg(test)]
#[path = "windows_console_tests.rs"]
mod tests;
