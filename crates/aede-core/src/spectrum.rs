//! Spectrogram pictures, drawn by ffmpeg, one per track.
//!
//! A spectrogram is the last arbiter when the provenance of a file is in
//! doubt: a lossless container filled from an MP3 shows a wall at 16 kHz that
//! no tag will ever mention. Aède does not decode and will not draw one
//! itself — it hands the file to ffmpeg and puts the picture where the person
//! looking for it will find it, beside the music.
//!
//! **The filter, the size and the layout are FlacCompagnon's**, deliberately
//! and to the character. The two programs are used together on the same
//! library, and a spectrogram that differed in scale, gain or colour map from
//! one tool to the other would be unreadable *as a pair* — the whole point of
//! looking at two is to compare them.
//!
//! The folder is the one thing that does not match: it is `spectrograms`, in
//! English like everything else here, where FlacCompagnon writes `spectres`.
//! Matching a *picture* is what makes the pair comparable; matching a *folder
//! name* buys nothing, and a French word in an otherwise English codebase is a
//! seam nobody would guess at.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::tags::AudioProperties;

/// Folder written beside the audio, holding one picture per track.
pub const FOLDER: &str = "spectrograms";

/// The ffmpeg filter, character for character as FlacCompagnon draws it.
///
/// `legend=1` draws the labelled frequency axis, whose top is the Nyquist
/// limit — without it the picture is pretty and says nothing, because there is
/// no way to tell 16 kHz from 22 kHz by eye.
const FILTER: &str =
    "showspectrumpic=s=1800x940:mode=combined:legend=1:color=intensity:scale=log:gain=3";

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
    caption: Option<&str>,
) -> Result<(), String> {
    if let Some(parent) = picture.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let with_text = caption.map(|text| {
        format!(
            "{FILTER},drawtext=text='{text}':fontcolor=white:fontsize=24:\
             x=14:y=12:box=1:boxcolor=black@0.55"
        )
    });
    if let Some(filter) = &with_text
        && run(ffmpeg, audio, filter, picture).is_ok()
    {
        return Ok(());
    }
    run(ffmpeg, audio, FILTER, picture)
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
mod tests {
    use super::*;

    fn properties(codec: &str, bits: Option<u16>) -> AudioProperties {
        AudioProperties {
            codec: codec.to_string(),
            sample_rate: Some(44_100),
            bit_depth: bits,
            channels: Some(2),
            bitrate_kbps: Some(320),
            ..Default::default()
        }
    }

    #[test]
    fn a_picture_sits_in_a_folder_of_its_own_beside_its_track() {
        let got = picture_for(Path::new("/m/Album/01 So What.flac"));
        assert_eq!(got, Path::new("/m/Album/spectrograms/01 So What.png"));
        // A name with no extension still gets a picture rather than a panic.
        assert_eq!(
            picture_for(Path::new("/m/Album/oddity")),
            Path::new("/m/Album/spectrograms/oddity.png")
        );
    }

    #[test]
    fn the_caption_says_what_the_file_claims_to_be() {
        assert_eq!(
            caption(&properties("flac", Some(16))),
            "44100 Hz | 16-bit | 2 ch | FLAC | Nyquist 22050 Hz"
        );
        // A lossy codec has no bit depth at all, and saying "float" there —
        // which a tool that only ever sees lossless files can afford — would
        // state something untrue about every MP3 in the library.
        assert_eq!(
            caption(&properties("mp3", None)),
            "44100 Hz | 320 kbps | 2 ch | MP3 | Nyquist 22050 Hz"
        );
    }

    /// Every character that could end the `drawtext` argument, start another
    /// filter, or escape out of the expression must be dropped. The codec name
    /// is read from the file, so it is attacker-chosen: a file declaring a
    /// codec of `x'a,b` would otherwise inject into the filter graph ffmpeg is
    /// handed. This is the test the character filter exists for — widen that
    /// set and it fails here rather than in somebody's music folder.
    #[test]
    fn the_caption_cannot_break_out_of_the_filter_graph() {
        let hostile = "A'B\"C:D,E\\F;G=H[I]J{K}L`M$N\nO\rP\tQ%R*S?T<U>V&W(X)";
        let text = caption(&properties(hostile, Some(16)));
        for bad in [
            '\'', '"', ':', ',', '\\', ';', '=', '[', ']', '{', '}', '`', '$', '\n', '\r', '\t',
            '%', '*', '?', '<', '>', '&', '(', ')',
        ] {
            assert!(!text.contains(bad), "{bad:?} survived in {text:?}");
        }
        assert!(text.contains("ABCDEF"), "and the letters remain: {text:?}");
    }

    #[test]
    fn a_caption_survives_a_file_that_declares_nonsense() {
        // The numbers come from a container that may be malformed. Nothing
        // here may divide by zero or panic.
        let mut p = properties("flac", Some(16));
        p.sample_rate = Some(0);
        p.channels = None;
        assert!(caption(&p).contains("Nyquist 0 Hz"));
        for weird in ["é", "日本語", "🎵", "\u{0}"] {
            let text = caption(&properties(weird, Some(16)));
            assert!(text.is_ascii(), "{text:?}");
            assert!(text.starts_with("44100 Hz"), "{text:?}");
        }
    }

    #[test]
    fn a_missing_picture_is_drawn_and_a_fresh_one_is_left_alone() {
        let dir = std::env::temp_dir().join("aede_spectrum_dates");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let audio = dir.join("one.flac");
        let picture = dir.join("one.png");
        std::fs::write(&audio, b"music").unwrap();
        assert!(out_of_date(&audio, &picture), "nothing drawn yet");

        std::fs::write(&picture, b"x").unwrap();
        assert!(
            !out_of_date(&audio, &picture),
            "drawn after the music was written"
        );

        // The music moves on: the picture is now of bytes nobody has any more.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(120);
        std::fs::File::options()
            .write(true)
            .open(&audio)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(out_of_date(&audio, &picture), "the track changed since");

        // A track that cannot be read at all is not evidence of a change, and
        // must not make every run redraw it.
        assert!(!out_of_date(&dir.join("gone.flac"), &picture));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
