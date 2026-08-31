//! Converting audio on the way out, by driving ffmpeg.
//!
//! **ffmpeg is an external program, not a dependency.** It is looked for, run,
//! and its absence reported in a sentence rather than as a mysterious failure.
//! The rule in `CLAUDE.md` about adding crates is not being bent here: nothing
//! is linked, nothing is vendored, and a build without ffmpeg installed is a
//! build where every other command still works.
//!
//! This is the first thing in the project that **writes audio**, and the line is
//! worth guarding: it writes new files, under a folder the user named, and it
//! never opens a source file for writing. Writing tags into a file produced here
//! is not the tag-rewriting the project refuses — that refusal protects the
//! user's own files, whose modification date, integrity verdict and scan state
//! all depend on not being touched. A file that did not exist a second ago has
//! none of those.

use std::path::Path;
use std::process::Command;

/// A format a copy can be converted into.
///
/// Deliberately short. Every entry here is a format a portable player is likely
/// to want, and each one costs a decoder somebody has to have: adding a target
/// nobody asked for is a promise to keep it working.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The one every device plays.
    Mp3,
    /// Best quality per byte at low bitrates, and what a modern player wants.
    Opus,
    /// What Apple devices prefer, in an MP4 container.
    Aac,
    /// The free lossy format Ogg carries.
    Vorbis,
    /// Lossless, and much smaller than WAV: the reason to convert a WAV rip.
    Flac,
    /// Uncompressed, for a device that plays nothing else.
    Wav,
}

/// One row per target: how it is typed, what it produces, and how it is asked
/// for on the command line.
///
/// A table rather than a `match` in four places, for the reason the role
/// vocabulary is one: a format added in one place and forgotten in another is
/// a target that half-works.
const TARGETS: &[(&str, Target, &str, &str, bool)] = &[
    // typed, target, extension, ffmpeg codec, lossless
    ("mp3", Target::Mp3, "mp3", "libmp3lame", false),
    ("opus", Target::Opus, "opus", "libopus", false),
    ("aac", Target::Aac, "m4a", "aac", false),
    ("m4a", Target::Aac, "m4a", "aac", false),
    ("vorbis", Target::Vorbis, "ogg", "libvorbis", false),
    ("ogg", Target::Vorbis, "ogg", "libvorbis", false),
    ("flac", Target::Flac, "flac", "flac", true),
    ("wav", Target::Wav, "wav", "pcm_s16le", true),
];

impl Target {
    /// The target a typed word names, or `None` for a word that names none.
    pub fn parse(word: &str) -> Option<Target> {
        let wanted = crate::text::normalize(word);
        TARGETS
            .iter()
            .find(|(name, ..)| *name == wanted)
            .map(|(_, target, ..)| *target)
    }

    /// Every spelling accepted, for a message that offers what it refuses.
    pub fn names() -> String {
        let mut names: Vec<&str> = TARGETS.iter().map(|(name, ..)| *name).collect();
        names.dedup();
        names.join(", ")
    }

    /// The targets that have a quality setting at all.
    ///
    /// Read from the same table as everything else, so a format added tomorrow
    /// appears here without anybody remembering to add it — which is the whole
    /// reason the table exists.
    pub fn lossy_names() -> String {
        TARGETS
            .iter()
            .filter(|(_, _, _, _, lossless)| !lossless)
            .map(|(name, ..)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn row(self) -> (&'static str, &'static str, bool) {
        TARGETS
            .iter()
            .find(|(_, target, ..)| *target == self)
            .map(|(_, _, extension, codec, lossless)| (*extension, *codec, *lossless))
            .unwrap_or(("mp3", "libmp3lame", false))
    }

    /// The file extension a converted file carries, without its dot.
    pub fn extension(self) -> &'static str {
        self.row().0
    }

    /// `true` when the target keeps every sample it was given.
    pub fn lossless(self) -> bool {
        self.row().2
    }

    /// Typical bitrate in kbps, used only to estimate a size before the work is
    /// done. `None` for a lossless target, where the figure would be a fiction.
    fn nominal_kbps(self, quality: Option<Quality>) -> Option<u32> {
        if self.lossless() {
            return None;
        }
        if let Some(Quality::Bitrate(kbps)) = quality {
            return Some(kbps);
        }
        Some(match self {
            // V0 averages around 245 kbps on real music.
            Target::Mp3 => 245,
            Target::Opus => 128,
            Target::Aac => 192,
            Target::Vorbis => 192,
            _ => 192,
        })
    }
}

/// How hard the encoder should try, as the user asked for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    /// A variable-quality setting, `V0`–`V9` for MP3 or `q0`–`q10` for Vorbis:
    /// lower is better for MP3, higher is better for Vorbis, which is the
    /// encoders' own convention and not something to paper over.
    Variable(u32),
    /// A constant bitrate in kbps, written `192k`.
    Bitrate(u32),
}

impl Quality {
    /// Reads `V0`, `q6` or `192k`, and nothing else.
    ///
    /// A value that parses as none of the three is refused rather than quietly
    /// replaced by the default: an encoder silently running at the wrong
    /// setting produces files that are wrong in a way nobody notices until the
    /// card is full.
    pub fn parse(word: &str) -> Option<Quality> {
        let word = word.trim();
        if let Some(digits) = word
            .strip_prefix(['V', 'v', 'Q', 'q'])
            .and_then(|rest| rest.parse::<u32>().ok())
            && digits <= 10
        {
            return Some(Quality::Variable(digits));
        }
        let bitrate = word.strip_suffix(['k', 'K']).unwrap_or(word);
        bitrate
            .parse::<u32>()
            .ok()
            .filter(|kbps| (8..=2048).contains(kbps))
            .map(Quality::Bitrate)
    }

    /// How the setting is written, for a message that repeats what was typed.
    pub const FORMS: &'static str = "V0…V9 for MP3, q0…q10 for Vorbis, or a bitrate like 192k";
}

/// What ffmpeg should be told about quality, given the target.
fn quality_arguments(target: Target, quality: Option<Quality>) -> Vec<String> {
    match (target, quality) {
        // A lossless target has no quality knob worth exposing: FLAC's
        // compression level trades encoding time for a few percent, and PCM
        // has none at all.
        (t, _) if t.lossless() => Vec::new(),
        (_, Some(Quality::Bitrate(kbps))) => vec!["-b:a".into(), format!("{kbps}k")],
        (_, Some(Quality::Variable(level))) => vec!["-q:a".into(), level.to_string()],
        // The defaults are the ones each encoder is actually good at, not a
        // single number applied to all of them.
        (Target::Mp3, None) => vec!["-q:a".into(), "0".into()],
        (Target::Vorbis, None) => vec!["-q:a".into(), "6".into()],
        (Target::Opus, None) => vec!["-b:a".into(), "128k".into()],
        (Target::Aac, None) => vec!["-b:a".into(), "192k".into()],
        (_, None) => Vec::new(),
    }
}

/// Roughly what a converted file will weigh, for a report given before the work.
///
/// An estimate, and said to be one wherever it is shown. The alternative is to
/// answer "unknown" to "will this fit on my card", which is the one question
/// somebody filling a card is asking.
pub fn estimated_size(
    target: Target,
    quality: Option<Quality>,
    duration_ms: u64,
    source: u64,
) -> u64 {
    match target.nominal_kbps(quality) {
        // kbps is kilobits per second, so a millisecond holds kbps/8 bytes:
        // ms × kbps ÷ 8. Dividing by 8000 instead — seconds and bits confused
        // in one step — made every estimate a thousand times too small, and a
        // card that could "obviously" hold the whole library. A test caught it.
        Some(kbps) => duration_ms.saturating_mul(kbps as u64) / 8,
        // Lossless from lossless: FLAC lands around 60 % of PCM on real music,
        // and PCM from FLAC around the inverse.
        None if target == Target::Flac => source * 3 / 5,
        None => source * 2,
    }
}

/// Where ffmpeg is, or `None` when it is not installed.
///
/// Kept as a name here, and answered in [`crate::ffmpeg`]: `spectrum` drives
/// the same program for another purpose, and two searches that could disagree
/// about which ffmpeg is "the" ffmpeg is one too many.
pub fn find_ffmpeg() -> Option<String> {
    crate::ffmpeg::find()
}

/// What to tell somebody who has no ffmpeg, worded so they can act on it.
///
/// One wording for every feature that needs it: see [`crate::ffmpeg::missing`].
pub fn missing_ffmpeg() -> String {
    crate::ffmpeg::missing("--compress")
}

/// Converts one file, and reports what went wrong in words rather than in an
/// exit status.
///
/// The metadata travels: `-map_metadata 0` carries the tags across, and the
/// embedded cover is copied as a picture stream where the container takes one.
/// Neither is perfect — no two tag formats hold quite the same fields, and a
/// Vorbis comment with three values for one field cannot be an ID3 frame that
/// holds one — but losing the artist and the title on the way to a player is
/// not a trade anybody would accept, and ffmpeg's mapping is what beets relies
/// on for the same reason.
pub fn convert(
    ffmpeg: &str,
    source: &Path,
    destination: &Path,
    target: Target,
    quality: Option<Quality>,
) -> Result<(), String> {
    let (_, codec, _) = target.row();
    let mut command = Command::new(ffmpeg);
    command
        // Without this a run in a terminal can stop dead waiting for an answer
        // to a question nobody saw, in the middle of nine hundred files.
        .arg("-nostdin")
        .args(["-loglevel", "error"])
        .arg("-y")
        .arg("-i")
        .arg(source)
        .args(["-map", "0:a"]);

    // The cover, where the container holds one. `?` makes the stream optional,
    // so a file without art is converted rather than refused.
    if matches!(target, Target::Mp3 | Target::Aac | Target::Flac) {
        command.args([
            "-map",
            "0:v?",
            "-c:v",
            "copy",
            "-disposition:v:0",
            "attached_pic",
        ]);
    }
    command
        .args(["-map_metadata", "0"])
        .args(["-c:a", codec])
        .args(quality_arguments(target, quality))
        .arg(destination);

    let output = command
        .output()
        .map_err(|e| format!("ffmpeg could not be run: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    // ffmpeg's own words, trimmed to the last few lines: the first ones are
    // usually the banner, and the reason is at the end.
    let said = String::from_utf8_lossy(&output.stderr);
    let reason: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = reason
        .iter()
        .rev()
        .take(2)
        .rev()
        .copied()
        .collect::<Vec<_>>()
        .join("; ");
    Err(match tail.is_empty() {
        true => "ffmpeg failed without saying why".to_string(),
        false => format!("ffmpeg: {tail}"),
    })
}

/// Longest a converted file's playing time may differ from its source.
///
/// Encoders pad and trim: an MP3 frame is 1152 samples and the encoder delay is
/// real, so an exact match would fail on correct output. Two seconds is far
/// larger than any of that and far smaller than a truncated encode.
const DURATION_TOLERANCE_MS: u64 = 2_000;

/// Checks a converted file the only way a converted file can be checked.
///
/// Comparing checksums is meaningless here: the bytes are different by
/// construction, which is the whole point. What can be checked is that the
/// result is a file this program can read, that it holds audio, and that the
/// audio lasts as long as what went in — which is what catches the failure that
/// actually happens, an encode cut short by a full disk or a killed process.
///
/// It reads the output with Aède's own parsers rather than asking ffmpeg again:
/// a verification that trusts the tool being verified is not one.
pub fn verify(destination: &Path, expected_ms: Option<u64>) -> Result<(), String> {
    let tags = crate::tags::read(destination)
        .map_err(|e| format!("what was written cannot be read back: {e}"))?;
    let Some(written) = tags.properties.duration_ms else {
        return Err("what was written has no readable duration".into());
    };
    if written == 0 {
        return Err("what was written holds no audio".into());
    }
    let Some(expected) = expected_ms.filter(|ms| *ms > 0) else {
        return Ok(());
    };
    if written.abs_diff(expected) > DURATION_TOLERANCE_MS {
        return Err(format!(
            "the result lasts {} where the source lasts {}: the encode was cut short",
            crate::text::format_duration(written),
            crate::text::format_duration(expected)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_is_named_the_way_a_person_would_name_it() {
        assert_eq!(Target::parse("mp3"), Some(Target::Mp3));
        assert_eq!(Target::parse("MP3"), Some(Target::Mp3));
        // Two words for one thing, because a user asking for "aac" and one
        // asking for "m4a" mean the same file.
        assert_eq!(Target::parse("aac"), Some(Target::Aac));
        assert_eq!(Target::parse("m4a"), Some(Target::Aac));
        assert_eq!(Target::parse("ogg"), Some(Target::Vorbis));
        assert_eq!(Target::parse("wma"), None);
        assert_eq!(
            Target::Aac.extension(),
            "m4a",
            "the container, not the codec"
        );
    }

    #[test]
    fn a_quality_that_parses_as_nothing_is_refused() {
        // An encoder silently running at a setting nobody asked for produces
        // files that are wrong in a way nobody notices until the card is full.
        assert_eq!(Quality::parse("V0"), Some(Quality::Variable(0)));
        assert_eq!(Quality::parse("v2"), Some(Quality::Variable(2)));
        assert_eq!(Quality::parse("q6"), Some(Quality::Variable(6)));
        assert_eq!(Quality::parse("192k"), Some(Quality::Bitrate(192)));
        assert_eq!(Quality::parse("320"), Some(Quality::Bitrate(320)));
        assert_eq!(Quality::parse("best"), None);
        assert_eq!(Quality::parse("V99"), None, "no such level");
        assert_eq!(Quality::parse("1k"), None, "not a bitrate anybody means");
    }

    #[test]
    fn each_encoder_is_asked_in_its_own_terms() {
        // MP3 counts down and Vorbis counts up; flattening the two into one
        // number would give somebody asking for the best Vorbis the worst one.
        assert_eq!(
            quality_arguments(Target::Mp3, None),
            vec!["-q:a".to_string(), "0".to_string()]
        );
        assert_eq!(
            quality_arguments(Target::Vorbis, None),
            vec!["-q:a".to_string(), "6".to_string()]
        );
        assert_eq!(
            quality_arguments(Target::Opus, None),
            vec!["-b:a".to_string(), "128k".to_string()]
        );
        assert_eq!(
            quality_arguments(Target::Mp3, Some(Quality::Bitrate(320))),
            vec!["-b:a".to_string(), "320k".to_string()]
        );
        // A lossless target has no knob, so it is given none rather than one
        // that would be ignored.
        assert!(quality_arguments(Target::Flac, Some(Quality::Bitrate(320))).is_empty());
    }

    #[test]
    fn a_size_is_estimated_from_the_playing_time() {
        // Four minutes at 128 kbps is about 3.8 MB, and the point of the figure
        // is to answer "will this fit" before an hour of encoding.
        let four_minutes = 240_000;
        let estimate = estimated_size(Target::Opus, None, four_minutes, 40_000_000);
        assert!((3_500_000..4_200_000).contains(&estimate), "got {estimate}");
        // And a bitrate that was asked for is the one used.
        let higher = estimated_size(Target::Opus, Some(Quality::Bitrate(256)), four_minutes, 0);
        assert!(higher > estimate * 3 / 2, "{higher} vs {estimate}");
    }
}
