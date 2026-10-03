//! Bounded password input; prompts use stderr so JSON output stays usable.

use std::io::{self, IsTerminal, Read, Write};

use super::super::Args;
use aede_core::accounts::MAX_PASSWORD_BYTES;

#[cfg(unix)]
#[path = "password_terminal_unix.rs"]
mod unix;
#[cfg(any(windows, test))]
#[path = "password_terminal_windows.rs"]
mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Char(char),
    Enter,
    Backspace,
    CtrlC,
    Escape,
    Tab,
    #[cfg(any(unix, test))]
    Ignored,
}

fn character_key(character: char) -> Key {
    match character {
        '\r' | '\n' => Key::Enter,
        '\u{8}' | '\u{7f}' => Key::Backspace,
        '\u{3}' => Key::CtrlC,
        '\u{1b}' => Key::Escape,
        '\t' => Key::Tab,
        character => Key::Char(character),
    }
}

pub(super) fn read(args: &Args) -> Result<String, Box<dyn std::error::Error>> {
    if args.has("password-stdin") {
        if io::stdin().is_terminal() {
            return Err(
                "--password-stdin requires redirected input; omit it for masked terminal entry"
                    .into(),
            );
        }
        return redirected(io::stdin().lock()).map_err(Into::into);
    }
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        return Err("use a terminal for masked password entry, or --password-stdin with redirected input for scripts".into());
    }
    #[cfg(unix)]
    let mut input = unix::Input::start()?;
    #[cfg(windows)]
    let mut input = windows::Input::start()?;
    #[cfg(not(any(unix, windows)))]
    return Err(
        "masked terminal input is unsupported on this platform; use --password-stdin".into(),
    );
    #[cfg(any(unix, windows))]
    {
        let mut terminal = io::stderr().lock();
        let result = confirmed(|prompt| {
            terminal.write_all(prompt.as_bytes())?;
            terminal.flush()?;
            #[cfg(unix)]
            let result = read_keys(|| input.read_key());
            #[cfg(windows)]
            let result = read_keys(|| input.read_character().map(character_key));
            terminal.write_all(b"\n")?;
            result
        });
        input.restore()?;
        result.map_err(Into::into)
    }
}

fn redirected(reader: impl Read) -> io::Result<String> {
    let mut password = String::new();
    reader
        .take(MAX_PASSWORD_BYTES as u64 + 3)
        .read_to_string(&mut password)?;
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    if password.len() > MAX_PASSWORD_BYTES || password.contains(['\r', '\n']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "provide one password line of at most 1024 UTF-8 bytes",
        ));
    }
    Ok(password)
}

fn confirmed(mut read_line: impl FnMut(&'static str) -> io::Result<String>) -> io::Result<String> {
    let password = read_line("Password: ")?;
    let confirmation = read_line("Confirm password: ")?;
    if password != confirmation {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "passwords do not match; no account changes were saved",
        ));
    }
    Ok(password)
}

fn read_keys(mut next: impl FnMut() -> io::Result<Key>) -> io::Result<String> {
    let mut password = String::new();
    // Consume an oversized line without allocating it or silently truncating
    // it. Backspace can remove its excess characters, and Ctrl-U clears it.
    let mut excess = 0usize;
    loop {
        match next()? {
            Key::Enter => {
                if excess != 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "password must contain at most 1024 UTF-8 bytes",
                    ));
                }
                return Ok(password);
            }
            Key::CtrlC | Key::Escape | Key::Char('\u{4}') => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "password entry cancelled; no account changes were saved",
                ));
            }
            Key::Backspace => {
                if excess != 0 {
                    excess -= 1;
                } else {
                    password.pop();
                }
            }
            Key::Char('\u{15}') => {
                password.clear();
                excess = 0;
            }
            Key::Char(character) if !character.is_control() => {
                if excess != 0 || password.len() + character.len_utf8() > MAX_PASSWORD_BYTES {
                    excess = excess.saturating_add(1);
                } else {
                    password.push(character);
                }
            }
            Key::Char(_) | Key::Tab => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "terminal passwords cannot contain control characters",
                ));
            }
            #[cfg(any(unix, test))]
            Key::Ignored => {}
        }
    }
}

#[cfg(test)]
#[path = "accounts_password_tests.rs"]
mod tests;
