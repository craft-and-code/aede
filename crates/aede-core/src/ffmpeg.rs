//! The one place that knows where ffmpeg is, and what to say when it is not.
//!
//! **ffmpeg is an external program, not a dependency.** Nothing is linked,
//! nothing is vendored, and a checkout without ffmpeg installed builds and
//! passes its tests — the features that need it say so and stop. Two commands
//! drive it today, `copy --compress` and `spectrum`, and the day a third
//! arrives it must not carry a third copy of the install instructions: a fact
//! hand-copied into three places will be right in two of them.
//!
//! The search is deliberately plain — `ffmpeg`, on the `PATH`. A GUI
//! application has to hunt through `/opt/homebrew/bin` and the rest because it
//! is launched by Finder and inherits no shell environment; a command run from
//! a terminal inherits the user's `PATH` by construction, and looking anywhere
//! else would only find a *different* ffmpeg from the one they get when they
//! type the name themselves.

use std::process::{Command, Stdio};

/// Where ffmpeg is, or `None` when it is not installed.
///
/// Looked for once per run by the caller, never once per file: a thousand
/// tracks would otherwise mean a thousand failed lookups before the first
/// picture is drawn.
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
mod tests {
    use super::*;

    #[test]
    fn the_message_names_what_wanted_it_and_how_to_get_it() {
        // "ffmpeg was not found" alone leaves the reader wondering what they
        // have lost and what to do about it.
        let text = missing("--compress");
        assert!(text.starts_with("--compress needs ffmpeg"), "{text}");
        assert!(text.contains("brew install ffmpeg"), "{text}");
        assert!(missing("spectrum").starts_with("spectrum needs ffmpeg"));
    }
}
