//! The one place that knows where ffmpeg is, and what to say when it is not.
//!
//! **ffmpeg is an external program, not a dependency.** Nothing is linked,
//! nothing is vendored, and a checkout without ffmpeg installed builds and
//! passes its tests — the features that need it say so and stop. Copying,
//! spectrum rendering and playback share these install instructions: a fact
//! hand-copied into several places will eventually be wrong in one of them.
//!
//! The search is deliberately plain — `ffmpeg`, on the `PATH`. A GUI
//! application has to hunt through `/opt/homebrew/bin` and the rest because it
//! is launched by Finder and inherits no shell environment; a command run from
//! a terminal inherits the user's `PATH` by construction, and looking anywhere
//! else would only find a *different* ffmpeg from the one they get when they
//! type the name themselves. Playback uses it for formats that the native
//! decoder cannot open, currently Opus, AAC and ALAC.

use std::process::{Command, Stdio};

/// Where ffmpeg is, or `None` when it is not installed.
///
/// Callers processing many files can retain this answer for their run.
pub fn find() -> Option<String> {
    let name = "ffmpeg";
    Command::new(name)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
        .then(|| name.to_string())
}

/// What to tell somebody who has no ffmpeg, worded so they can act on it.
///
/// `what` names the feature that wanted it, because "ffmpeg was not found" on
/// its own leaves the reader wondering what they have lost.
pub fn missing(what: &str) -> String {
    format!(
        "\
{what} needs ffmpeg, and it was not found.

ffmpeg is an external program, not something Aède ships: it does the decoding
and the drawing, Aède decides what to hand it. Everything else works without
it.

  macOS          brew install ffmpeg
  Debian/Ubuntu  sudo apt install ffmpeg
  Arch           sudo pacman -S ffmpeg
  Fedora         sudo dnf install ffmpeg"
    )
}

#[cfg(test)]
#[path = "ffmpeg_tests.rs"]
mod tests;
