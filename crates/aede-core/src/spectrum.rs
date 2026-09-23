//! Spectrogram pictures, drawn by ffmpeg, one per track.
//!
//! A spectrogram is the last arbiter when the provenance of a file is in
//! doubt: a lossless container filled from an MP3 shows a wall at 16 kHz that
//! no tag will ever mention. Aède does not decode and will not draw one
//! itself — it hands the file to ffmpeg and puts the picture where the person
//! looking for it will find it, beside the music.
//!
//! **The filter and the layout are FlacCompagnon's**, deliberately and to the
//! character. The two programs are used together on the same library, and a
//! spectrogram that differed in gain or colour map from one tool to the other
//! would be unreadable *as a pair* — the whole point of looking at two is to
//! compare them. The frame size no longer has to match: see [`Size`].
//! `--size full` still draws FlacCompagnon's own dimensions, character for
//! character, for whoever wants the two side by side; the default is half of
//! that, because most runs are not a side-by-side and a full-size picture is
//! a few megabytes a track.
//!
//! The folder is another thing that does not match: it is `spectrograms`, in
//! English like everything else here, where FlacCompagnon writes `spectres`.
//! Matching a *picture* is what makes the pair comparable; matching a *folder
//! name* buys nothing, and a French word in an otherwise English codebase is a
//! seam nobody would guess at.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tags::AudioProperties;

/// Folder written beside the audio, holding one picture per track.
pub const FOLDER: &str = "spectrograms";

/// How large a spectrogram picture is drawn.
///
/// [`Size::Full`] is FlacCompagnon's own frame, `1800x940`, for putting the
/// two tools' pictures side by side. [`Size::Half`] is the default: halving
/// both dimensions quarters the pixel count, and a spectrogram is mostly
/// noise, which a PNG encoder cannot compress away — so the picture shrinks
/// close to that same quarter instead of by half. A library of a few
/// thousand tracks stays in the megabytes rather than the gigabytes, and the
/// legend is still legible at this size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// `1800x940`, character for character what FlacCompagnon draws.
    Full,
    /// `900x470`. Used unless `--size full` is given.
    Half,
}

impl Size {
    /// Reads what a reader typed after `--size`.
    pub fn parse(text: &str) -> Option<Size> {
        match text.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Size::Full),
            "half" => Some(Size::Half),
            _ => None,
        }
    }

    /// The `WxH` fed to ffmpeg's `showspectrumpic`.
    fn dimensions(self) -> &'static str {
        match self {
            Size::Full => "1800x940",
            Size::Half => "900x470",
        }
    }

    /// How this size reads in a message.
    pub fn as_str(self) -> &'static str {
        match self {
            Size::Full => "full",
            Size::Half => "half",
        }
    }
}

/// The ffmpeg filter, character for character as FlacCompagnon draws it,
/// except for the frame size, which [`Size`] now chooses.
///
/// `legend=1` draws the labelled frequency axis, whose top is the Nyquist
/// limit — without it the picture is pretty and says nothing, because there is
/// no way to tell 16 kHz from 22 kHz by eye.
fn filter(size: Size) -> String {
    format!(
        "showspectrumpic=s={}:mode=combined:legend=1:color=intensity:scale=log:gain=3",
        size.dimensions()
    )
}

/// Where the picture of a file belongs: `<its folder>/spectrograms/<name>.png`.
pub fn picture_for(audio: &Path) -> PathBuf {
    let folder = audio.parent().unwrap_or_else(|| Path::new("."));
    let stem = audio
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("track");
    folder.join(FOLDER).join(format!("{stem}.png"))
}

/// `true` when the picture has to be drawn: it is missing, or it is older than
/// the music it describes.
///
/// Both dates are read from the **disk**, not from the catalog. The catalog
/// remembers when it last read a file, which is a different fact and the wrong
/// one here: the question is whether this picture was drawn from the bytes that
/// are there now, and a library edited since the last scan would otherwise keep
/// pictures of music nobody has any more.
///
/// The modification date rather than a checksum, because it is the same test
/// the incremental scan uses, and because a picture is cheap to redraw and
/// expensive to verify. A run over a library that has not changed draws nothing
/// at all, which is what makes the command safe to repeat.
pub fn out_of_date(audio: &Path, picture: &Path) -> bool {
    let Some(drawn) = modified(picture) else {
        return true;
    };
    // A track whose date cannot be read is left alone rather than redrawn on
    // every run: an unreadable date is not evidence that anything changed.
    modified(audio).is_some_and(|music| drawn < music)
}

fn modified(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// The line drawn across the top of the picture.
///
/// The legend already carries the frequency axis; this says in words what the
/// file claims to be, so that a picture kept on its own — sent to somebody,
/// looked at a year later — still answers "at what sample rate?".
///
/// The whole string is embedded in an ffmpeg `drawtext` expression, and part of
/// it comes from the file itself, so it is restricted to a character set that
/// cannot end the argument, start another filter, or escape the expression.
/// See the test: this is the one function here with an attacker on the other
/// side of it.
pub fn caption(properties: &AudioProperties) -> String {
    let depth = match (properties.bit_depth, properties.bitrate_kbps) {
        (Some(bits), _) => format!("{bits}-bit"),
        // A lossy codec has no bit depth at all — printing "float" there, as a
        // tool that only ever sees lossless files can afford to, would state
        // something untrue about every MP3 in the library.
        (None, Some(kbps)) => format!("{kbps} kbps"),
        (None, None) => "unknown depth".to_string(),
    };
    let rate = properties.sample_rate.unwrap_or(0);
    let raw = format!(
        "{rate} Hz | {depth} | {} ch | {} | Nyquist {} Hz",
        properties.channels.unwrap_or(0),
        properties.codec.to_uppercase(),
        rate / 2,
    );
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '|' | '-' | '/' | '.'))
        .collect()
}

/// Draws the picture for one file.
///
/// Two attempts: with the caption, then without. `drawtext` needs a font, and
/// an installation that has none fails the whole filter graph — losing the
/// picture over a line of text would be a poor trade, and the legend still
/// carries the axis that matters.
pub fn render(
    ffmpeg: &str,
    audio: &Path,
    picture: &Path,
    size: Size,
    caption: Option<&str>,
) -> Result<(), String> {
    if let Some(parent) = picture.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let base = filter(size);
    let with_text = caption.map(|text| {
        format!(
            "{base},drawtext=text='{text}':fontcolor=white:fontsize=24:\
             x=14:y=12:box=1:boxcolor=black@0.55"
        )
    });
    if let Some(with_text) = &with_text
        && run(ffmpeg, audio, with_text, picture).is_ok()
    {
        return Ok(());
    }
    run(ffmpeg, audio, &base, picture)
}

fn run(ffmpeg: &str, audio: &Path, filter: &str, picture: &Path) -> Result<(), String> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(audio)
        .args(["-lavfi", filter, "-frames:v", "1"])
        .arg(picture)
        .output()
        .map_err(|e| format!("ffmpeg could not be run: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let last: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
    Err(last
        .iter()
        .rev()
        .take(2)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join(" — "))
}

#[cfg(test)]
#[path = "spectrum_tests.rs"]
mod tests;
