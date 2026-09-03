//! The acoustic fingerprint of a file: what the audio *is*, not what it says.
//!
//! # Why this exists
//!
//! Every other identifier in this program comes out of the tags, and a badly
//! tagged file therefore cannot be identified at all: `track03.mp3` with no
//! title and no artist is invisible to MusicBrainz, because there is nothing
//! to ask about. A fingerprint is computed from the **decoded audio**, so it
//! answers for a file whose tags say nothing, and it disagrees with a file
//! whose tags say the wrong thing.
//!
//! # It is computed, not fetched — and stored beside the checksum
//!
//! A fingerprint is Aède's own conclusion about the bytes, like
//! [`crate::audit::integrity`] and unlike anything in `sources.json`: nobody
//! told us, we worked it out. So it lives on [`crate::model::AudioFile`] and
//! is carried across a rescan by the same size-and-mtime rule the integrity
//! verdict uses. What **AcoustID** answers when shown one is a different
//! thing entirely, and goes in the attributed layer where every other
//! source's claim goes.
//!
//! # Two ways to compute one, and why ffmpeg comes first
//!
//! Chromaprint is the algorithm; there are two ordinary ways to run it.
//!
//! - **ffmpeg**, *when* it was built with `--enable-chromaprint`. Debian and
//!   Ubuntu build it that way, so on those machines this is no new dependency
//!   at all — Aède already requires ffmpeg for `spectrum` and
//!   `copy --compress`.
//! - **`fpcalc`**, the tool Chromaprint itself ships. Canonical, and the one
//!   AcoustID's own documentation names.
//!
//! **Homebrew's ffmpeg does not have chromaprint**, which this module's first
//! version claimed it did — a guess written as a fact, corrected by a reader
//! pasting the `configuration:` line of a fresh `brew install ffmpeg`. On
//! macOS the answer is `brew install chromaprint`, and it is a small package.
//! The lesson is not about ffmpeg: **a build option is a property of somebody
//! else's packaging decision, and this program has no business asserting one
//! it has not seen.** So the availability check asks the muxer list rather
//! than reading a version string, and the message names both ways out with
//! the right one first for each platform.
//!
//! ffmpeg is tried first because it is more likely to be there already; the
//! fallback exists because "built with chromaprint" is not a promise anyone
//! made. **Neither is linked or vendored**, exactly as ffmpeg is not: a
//! checkout with neither installed builds, passes its tests, and says plainly
//! what is missing.
//!
//! # The duration is part of the question
//!
//! AcoustID is asked for a fingerprint *and* a length, and refuses without
//! both — the length is how it tells a full track from an excerpt that
//! fingerprints alike. `fpcalc` prints it; ffmpeg's muxer does not, so the
//! caller supplies it from the catalog, which read it out of the file's own
//! header at scan time.

use std::process::Command;

/// What one file's audio amounts to, for a service that identifies audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    /// The base64 compressed fingerprint, as both tools emit it.
    pub data: String,
    /// The track's length in whole seconds, which the lookup also needs.
    pub seconds: u32,
}

/// Which program produced a fingerprint, for a message to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    /// ffmpeg's `chromaprint` muxer.
    Ffmpeg,
    /// Chromaprint's own `fpcalc`.
    Fpcalc,
}

impl By {
    /// The program's name, as somebody would type it.
    pub fn program(self) -> &'static str {
        match self {
            By::Ffmpeg => "ffmpeg",
            By::Fpcalc => "fpcalc",
        }
    }
}

/// The algorithm AcoustID's index is built on.
///
/// Chromaprint has three, and a fingerprint computed with the wrong one is not
/// wrong-looking — it is a valid fingerprint that matches nothing, which is
/// indistinguishable from a recording nobody has ever submitted. `fpcalc`
/// defaults to it; ffmpeg's muxer numbers the same list from zero, so the
/// second entry is the one, and it is passed explicitly rather than trusted to
/// stay the default.
const ALGORITHM: &str = "1";

/// Whether either program is available, and which.
///
/// Looked for once per run by the caller, never once per file: a thousand
/// tracks would otherwise mean a thousand failed lookups before the first
/// fingerprint. The same reasoning, and the same shape, as
/// [`crate::ffmpeg::find`].
pub fn find() -> Option<By> {
    if has_chromaprint() {
        return Some(By::Ffmpeg);
    }
    Command::new("fpcalc")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
        .then_some(By::Fpcalc)
}

/// `true` when the ffmpeg on this machine can produce a fingerprint.
///
/// Asked of its muxer list rather than of its version string: a build says
/// `--enable-chromaprint` in its configuration line only when it was compiled
/// from source with that flag showing, while the muxer list is what it can
/// actually do.
fn has_chromaprint() -> bool {
    let Some(ffmpeg) = crate::ffmpeg::find() else {
        return false;
    };
    Command::new(ffmpeg)
        .args(["-hide_banner", "-muxers"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("chromaprint"))
}

/// Computes the fingerprint of one file.
///
/// `seconds` is the length the catalog holds, used only on the ffmpeg path,
/// which emits no duration of its own. `fpcalc` states one and its answer
/// wins: it measured what it actually decoded, and a header that disagrees
/// with the stream is exactly the kind of file this whole feature exists for.
pub fn of(by: By, path: &std::path::Path, seconds: u32) -> Result<Fingerprint, String> {
    match by {
        By::Ffmpeg => {
            let ffmpeg = crate::ffmpeg::find().ok_or_else(|| "ffmpeg is gone".to_string())?;
            let out = Command::new(ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-i"])
                .arg(path)
                .args(["-f", "chromaprint", "-fp_format", "base64"])
                .args(["-algorithm", ALGORITHM])
                .arg("-")
                .output()
                .map_err(|e| format!("ffmpeg could not be run: {e}"))?;
            if !out.status.success() {
                return Err(said(&out.stderr));
            }
            read_ffmpeg(&String::from_utf8_lossy(&out.stdout), seconds)
        }
        By::Fpcalc => {
            let out = Command::new("fpcalc")
                .arg(path)
                .output()
                .map_err(|e| format!("fpcalc could not be run: {e}"))?;
            if !out.status.success() {
                return Err(said(&out.stderr));
            }
            read_fpcalc(&String::from_utf8_lossy(&out.stdout))
        }
    }
}

/// The last thing a failed program said, for a message worth reading.
fn said(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .rfind(|line| !line.trim().is_empty())
        .unwrap_or("it failed without saying why")
        .to_string()
}

/// Reads what ffmpeg's chromaprint muxer wrote to its output.
///
/// Split out so the parsing is tested without running anything: the muxer
/// writes the base64 fingerprint and nothing else, and the empty answer — a
/// file too short to fingerprint, or one that decoded to silence — has to be
/// a refusal rather than an empty fingerprint sent to a service that would
/// match it against everything.
pub fn read_ffmpeg(stdout: &str, seconds: u32) -> Result<Fingerprint, String> {
    let data = stdout.trim().to_string();
    usable(data, seconds)
}

/// Reads `fpcalc`'s two lines: `DURATION=` and `FINGERPRINT=`.
///
/// The duration it states is preferred over the caller's, because it is the
/// length of what was decoded rather than the length a header claims.
pub fn read_fpcalc(stdout: &str) -> Result<Fingerprint, String> {
    let mut data = String::new();
    let mut seconds = 0u32;
    for line in stdout.lines() {
        match line.split_once('=') {
            Some(("FINGERPRINT", value)) => data = value.trim().to_string(),
            Some(("DURATION", value)) => {
                // fpcalc prints a whole number, but a build that printed
                // `183.4` must not silently become nothing at all.
                seconds = value
                    .trim()
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .parse()
                    .unwrap_or(0);
            }
            _ => {}
        }
    }
    usable(data, seconds)
}

/// The one gate both readers pass through.
///
/// An empty fingerprint and a zero length are each refused, because either
/// makes a lookup that cannot mean anything: AcoustID needs both, and sending
/// a blank one asks the service to match silence against its whole index.
fn usable(data: String, seconds: u32) -> Result<Fingerprint, String> {
    if data.is_empty() {
        return Err("no fingerprint came back: the audio may be too short".to_string());
    }
    if seconds == 0 {
        return Err("no duration is known for this file, and a lookup needs one".to_string());
    }
    Ok(Fingerprint { data, seconds })
}

/// What to tell somebody who has neither program, worded so they can act.
///
/// Both ways out are named, and which one is likelier is said, because "no
/// fingerprinting tool was found" leaves a reader to guess that ffmpeg — which
/// they already have for the spectrograms — might be the answer.
pub fn missing() -> String {
    "\
Fingerprinting needs Chromaprint, and no way to run it was found.

Two programs can do it, and Aède ships neither.

  macOS      brew install chromaprint
             Homebrew's ffmpeg is built without chromaprint, so the ffmpeg
             you already have for the spectrograms cannot do this one.

  Debian     apt install ffmpeg
  Ubuntu     — theirs is built with chromaprint, so this is enough. Failing
             that, apt install libchromaprint-tools.

Everything else in Aède works without either. Run \"aede fingerprint\" again
once one of them is on your PATH."
        .to_string()
}

#[cfg(test)]
#[path = "fingerprint_tests.rs"]
mod tests;
