//! A local PCM output session that can span several decoded files.
//!
//! The session owns the output process, not the queue or decoder. A caller may
//! keep it across tracks in a local player. Raw `f32le` input has one
//! fixed rate and channel count per process; a format change drains the old
//! process before opening another one. Resampling is a separate concern.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};

pub use super::format::PcmStreamFormat as OutputFormat;

/// What happened when a track's PCM format reached the output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputChange {
    /// An empty session opened its first process.
    Opened,
    /// The current process accepts this format and remains open.
    Reused,
    /// A different rate or channel count required a new process.
    ReopenedForFormat,
}

/// Failure to describe, start, or use the local output.
#[derive(Debug)]
pub enum OutputError {
    /// A raw PCM stream must name its speaker positions explicitly.
    InvalidLayout,
    /// The output process could not start.
    Start(io::Error),
    /// The started process did not provide a writable PCM input.
    MissingInput,
    /// Writing, stopping, or waiting for the process failed.
    Device(io::Error),
    /// The output process reported an unsuccessful exit.
    Exit(std::process::ExitStatus),
}

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout => f.write_str("audio output needs a known channel layout"),
            Self::Start(error) => write!(
                f,
                "cannot start ffplay: {error}; install ffplay for local audio output"
            ),
            Self::MissingInput => f.write_str("ffplay did not open its PCM input"),
            Self::Device(error) => write!(f, "audio output failed: {error}"),
            Self::Exit(status) => write!(
                f,
                "ffplay stopped with {status}; check the audio output device"
            ),
        }
    }
}

impl std::error::Error for OutputError {}

/// One output process for each uninterrupted run of a PCM format.
///
/// A natural track transition reuses the process. The caller should call
/// `abort` when seeking or skipping, because ffplay may have queued samples
/// from the old track. `finish` closes its input and waits for the last audio
/// to drain. Dropping an active session stops its child process.
pub struct OutputSession {
    program: PathBuf,
    process: Option<Child>,
    input: Option<ChildStdin>,
    format: Option<OutputFormat>,
}

impl Default for OutputSession {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputSession {
    /// Use `ffplay` from the local executable search path.
    pub fn new() -> Self {
        Self::with_program(PathBuf::from("ffplay"))
    }

    fn with_program(program: PathBuf) -> Self {
        Self {
            program,
            process: None,
            input: None,
            format: None,
        }
    }

    /// The format currently accepted by the process, if any.
    pub fn format(&self) -> Option<OutputFormat> {
        self.format
    }

    /// Process identifier used by the terminal transport controls.
    pub fn process_id(&self) -> Option<u32> {
        self.process.as_ref().map(Child::id)
    }

    /// Reuse matching output or drain and reopen it for a new format.
    pub fn prepare(&mut self, format: OutputFormat) -> Result<OutputChange, OutputError> {
        let layout = format
            .layout()
            .ffmpeg_name()
            .ok_or(OutputError::InvalidLayout)?;
        if self.format == Some(format) && self.input.is_some() && !self.poll_finished()? {
            return Ok(OutputChange::Reused);
        }
        let changed = self.format.is_some_and(|held| held != format);
        self.finish()?;
        let mut command = Command::new(&self.program);
        command
            .args([
                OsString::from("-hide_banner"),
                OsString::from("-loglevel"),
                OsString::from("error"),
                OsString::from("-nodisp"),
                OsString::from("-autoexit"),
                OsString::from("-nostats"),
                OsString::from("-f"),
                OsString::from("f32le"),
                OsString::from("-sample_rate"),
                OsString::from(format.sample_rate().to_string()),
                OsString::from("-ch_layout"),
                OsString::from(layout),
                OsString::from("-i"),
                OsString::from("pipe:0"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null());
        let mut process = command.spawn().map_err(OutputError::Start)?;
        let Some(input) = process.stdin.take() else {
            let _ = process.kill();
            let _ = process.wait();
            return Err(OutputError::MissingInput);
        };
        self.process = Some(process);
        self.input = Some(input);
        self.format = Some(format);
        Ok(if changed {
            OutputChange::ReopenedForFormat
        } else {
            OutputChange::Opened
        })
    }

    /// Close the input while allowing the process to play queued samples.
    pub fn close_input(&mut self) {
        drop(self.input.take());
    }

    /// Return true after a successful child exit, or an error for failure.
    pub fn poll_finished(&mut self) -> Result<bool, OutputError> {
        let Some(process) = self.process.as_mut() else {
            return Ok(true);
        };
        let Some(status) = process.try_wait().map_err(OutputError::Device)? else {
            return Ok(false);
        };
        drop(self.input.take());
        drop(self.process.take());
        self.format = None;
        if !status.success() {
            return Err(OutputError::Exit(status));
        }
        Ok(true)
    }

    /// Close the input and wait for the output process to drain normally.
    pub fn finish(&mut self) -> Result<(), OutputError> {
        self.close_input();
        let Some(mut process) = self.process.take() else {
            self.format = None;
            return Ok(());
        };
        let status = process.wait().map_err(OutputError::Device)?;
        self.format = None;
        if !status.success() {
            return Err(OutputError::Exit(status));
        }
        Ok(())
    }

    /// Discard queued output and stop the process after a transport change.
    pub fn abort(&mut self) -> Result<(), OutputError> {
        self.close_input();
        let Some(mut process) = self.process.take() else {
            self.format = None;
            return Ok(());
        };
        let result = process.kill();
        let waited = process.wait();
        self.format = None;
        if let Err(error) = result
            && error.kind() != io::ErrorKind::InvalidInput
        {
            return Err(OutputError::Device(error));
        }
        waited.map_err(OutputError::Device)?;
        Ok(())
    }
}

impl Write for OutputSession {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.input
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "audio output is closed"))?
            .write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.input
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "audio output is closed"))?
            .flush()
    }
}

impl Drop for OutputSession {
    fn drop(&mut self) {
        let _ = self.abort();
    }
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
