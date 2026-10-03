//! Scoped Unix terminal modes shared by playback controls and password entry.

use std::io;
use std::process::{Command, Stdio};

pub(super) struct TerminalMode {
    original: Option<String>,
}

impl TerminalMode {
    pub(super) fn start(arguments: &[&str]) -> io::Result<Self> {
        let original = Command::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()?;
        if !original.status.success() {
            return Err(io::Error::other("cannot read terminal mode with stty"));
        }
        let original = String::from_utf8(original.stdout)
            .map_err(|_| io::Error::other("stty returned an invalid terminal mode"))?;
        let mut mode = Self {
            original: Some(original.trim().to_owned()),
        };
        let changed = Command::new("stty")
            .args(arguments)
            .stdin(Stdio::inherit())
            .output();
        if !changed?.status.success() {
            // The guard also restores on partial failure or an I/O error.
            mode.restore()?;
            return Err(io::Error::other("cannot change terminal mode with stty"));
        }
        Ok(mode)
    }

    pub(super) fn restore(&mut self) -> io::Result<()> {
        if let Some(original) = &self.original {
            let restored = Command::new("stty")
                .arg(original)
                .stdin(Stdio::inherit())
                .output()?;
            if !restored.status.success() {
                return Err(io::Error::other("cannot restore terminal mode with stty"));
            }
            self.original = None;
        }
        Ok(())
    }

    /// Keep the current mode if a caller cannot safely re-enable echo.
    pub(super) fn retain_current(&mut self) {
        self.original = None;
    }
}

impl Drop for TerminalMode {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("{error}");
        }
    }
}
