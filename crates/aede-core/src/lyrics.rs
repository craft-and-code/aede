//! Lyrics that are already there: in the tags, or in an `.lrc` beside the file.
//!
//! Nothing here touches the network. Reading what a library already holds is
//! the first half of the job and the cheap one — the parsers walk past `USLT`,
//! `LYRICS` and `©lyr` today, and a `.lrc` sitting next to a track is a text
//! file anyone can open. Fetching what is *missing* is another matter
//! entirely: it needs a network, a source whose terms allow it, and an explicit
//! choice, because lyrics are the composition's copyright and owning a FLAC
//! grants no rights in it. See `docs/design/lyrics.md`.
//!
//! **Where they live follows the rule the rest of the catalog follows.** Tag
//! lyrics are already in it, because raw tags are kept per file; a sidecar is
//! not in the file, so pretending it were a tag would make the catalog lie
//! about what the file says. The catalog therefore stores the sidecar's
//! **path**, and the text is read from it when somebody asks — it is one small
//! file, sitting right next to the music it belongs to.
//!
//! [`Timeline`] indexes the parsed timings for terminal and graphical players.
//! The player supplies its decoded-track position; this module does not run a
//! playback clock or attach lyrics to audio packets.

use std::io::Read;
use std::path::{Path, PathBuf};

#[path = "lyrics_source.rs"]
mod source;
pub use source::{CurrentTrack, ReadError, read_current, read_local};

#[path = "lyrics_timeline.rs"]
mod timeline;
pub use timeline::{Cue, Timeline};

/// Extension of a lyrics file beside a track.
pub const EXTENSION: &str = "lrc";

/// At most this much of a lyrics file is read.
///
/// Lyrics are a page of text. A ten-megabyte `.lrc` is not a song, and reading
/// it whole to show a track page would be paying for somebody's mistake.
const LIMIT: usize = 256 * 1024;
/// Text storage after repeating lines for their timestamps.
const MIN_TEXT_BUDGET: usize = 1024 * 1024;

/// One line, with the moment it is sung when the file says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Milliseconds from the start of the track, or `None` on a plain line.
    ///
    /// Offsets in LRC input have already been applied. A player uses its
    /// decoded-track position, including a seek offset, to follow these times.
    pub at_ms: Option<u64>,
    /// The line as written, without its timestamp when it could be expanded safely.
    /// Invalid or excessive timestamps remain literal text on an untimed line.
    pub text: String,
}

/// Where a track's lyrics were found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A tag inside the audio file itself.
    Tag,
    /// A `.lrc` file beside it.
    Sidecar,
}

/// The lyrics of one track, as read.
#[derive(Debug, Clone, PartialEq)]
pub struct Lyrics {
    /// Where they came from, which the page names so the reader knows.
    pub source: Source,
    /// The file they were read from: the audio itself, or the sidecar.
    pub origin: String,
    /// Every line, in order.
    pub lines: Vec<Line>,
}

impl Lyrics {
    /// `true` when at least one line carries a time.
    ///
    /// Not "every line": real `.lrc` files carry untimed headers, blank lines
    /// and the occasional stray, and a file that is 95% timed is a synced file.
    pub fn synced(&self) -> bool {
        self.lines.iter().any(|line| line.at_ms.is_some())
    }

    /// The whole text, timestamps dropped — what a search looks through.
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Where the sidecar of an audio file would be: the same name, `.lrc`.
pub fn sidecar_of(audio: &Path) -> PathBuf {
    audio.with_extension(EXTENSION)
}

/// `true` when a file name is a lyrics sidecar, whatever its case.
pub fn is_sidecar(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".lrc")
}

/// Reads a sidecar, bounded, and never fatally.
///
/// Invalid UTF-8 is replaced rather than refused. A `.lrc` written on a Windows
/// machine in 2003 is Latin-1 as often as not, and refusing to show a song
/// because of one accent would be the wrong trade. Reads at most 256 KiB;
/// expanded line text stays within 1 MiB, including after lossy decoding.
pub fn read(path: &Path) -> Option<Lyrics> {
    let file = std::fs::File::open(path).ok()?;
    let text = read_text(file).ok()?;
    let lines = parse_with_budget(&text, MIN_TEXT_BUDGET);
    match lines.is_empty() {
        true => None,
        false => Some(Lyrics {
            source: Source::Sidecar,
            origin: path.to_string_lossy().to_string(),
            lines,
        }),
    }
}

fn read_text(reader: impl Read) -> std::io::Result<String> {
    let mut raw = Vec::new();
    reader.take(LIMIT as u64).read_to_end(&mut raw)?;
    Ok(String::from_utf8_lossy(&raw).into_owned())
}

/// The lyrics a tag carries, if it carries any.
pub fn from_tag(origin: &str, tag: &str) -> Option<Lyrics> {
    let lines = parse(tag);
    match lines.is_empty() {
        true => None,
        false => Some(Lyrics {
            source: Source::Tag,
            origin: origin.to_string(),
            lines,
        }),
    }
}

/// Reads lyrics text, timed or not.
///
/// One parser for both, because a tag can perfectly well hold LRC — plenty of
/// taggers write the synced text straight into `LYRICS`, and a reader that only
/// understood plain text would show a page of `[00:12.34]` to the user.
///
/// The `.lrc` metadata headers (`[ar:…]`, `[ti:…]`, `[by:…]`) are dropped: they
/// repeat what the tags already say, and this file is not where the artist's
/// name is settled. `[offset:…]` is applied, since it exists precisely to shift
/// a file that was timed against another encoding. An unrepresentable timestamp
/// remains literal, untimed text rather than hiding the line or overflowing.
/// Timed blank lines are kept, including at the end, so a player can clear the
/// preceding verse. Only untimed blank edges and an initial UTF-8 BOM are removed.
///
/// Repeated timestamps share a text budget of `max(1 MiB, 4 * text.len())`
/// bytes across the entire result. A line that cannot fit its expanded copies
/// remains one untimed literal line, preserving its words and timestamps.
pub fn parse(text: &str) -> Vec<Line> {
    parse_with_budget(text, MIN_TEXT_BUDGET.max(text.len().saturating_mul(4)))
}

/// A complete lyrics result would exceed the permitted UTF-8 text budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimitExceeded;

impl std::fmt::Display for ParseLimitExceeded {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("complete lyrics exceed the text budget")
    }
}

impl std::error::Error for ParseLimitExceeded {}

/// Parses lyrics completely, refusing excessive timestamp expansion.
///
/// Unlike [`parse`], this does not turn an over-budget timed chorus into
/// literal untimed text. This distinction lets an API promise complete timing
/// rather than silently downgrade it. Malformed timestamps remain literal
/// text under the same rules as the ordinary parser.
///
/// `text_budget` limits the combined UTF-8 bytes in all returned line texts,
/// including every expanded repetition; it does not include vector storage,
/// line separators or a transport's serialization overhead. Callers reading
/// untrusted sources must also bound their input and encoded response size.
pub fn parse_complete(text: &str, text_budget: usize) -> Result<Vec<Line>, ParseLimitExceeded> {
    let (lines, exceeded) = parse_bounded(text, text_budget, true);
    if exceeded {
        Err(ParseLimitExceeded)
    } else {
        Ok(lines)
    }
}

fn parse_with_budget(text: &str, budget: usize) -> Vec<Line> {
    parse_bounded(text, budget, false).0
}

fn parse_bounded(text: &str, budget: usize, complete: bool) -> (Vec<Line>, bool) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut offset_ms: i64 = 0;
    let mut lines = Vec::new();
    let mut used_text = 0usize;
    // Reserve the raw spelling of later lines, so an accepted chorus cannot
    // consume the space needed to retain subsequent words and timestamps.
    let mut remaining_raw = text
        .lines()
        .fold(0usize, |bytes, line| bytes.saturating_add(line.len()));
    for raw in text.lines() {
        remaining_raw = remaining_raw.saturating_sub(raw.len());
        let raw = raw.trim_end_matches('\r');
        if let Some(value) = header(raw, "offset") {
            offset_ms = value.trim().parse().unwrap_or(0);
            continue;
        }
        if is_header(raw) {
            continue;
        }
        let (times, rest) = timestamps(raw);
        if times.is_empty() {
            let text = rest.trim_end().to_string();
            // A blank line inside lyrics is a verse break and worth keeping;
            // one at the very start is just the file's shape.
            if !text.is_empty() || !lines.is_empty() {
                used_text = used_text.saturating_add(text.len());
                if complete && used_text > budget {
                    return (Vec::new(), true);
                }
                lines.push(Line { at_ms: None, text });
            }
            continue;
        }
        // `[00:12.00][01:44.00] the chorus again` is one line sung twice, and
        // the file means both: a chorus that only appeared once would leave a
        // player silent at its second turn.
        let text = rest.trim();
        let expanded = text.len().saturating_mul(times.len());
        let available = match complete {
            true => budget.saturating_sub(used_text),
            false => budget
                .saturating_sub(used_text)
                .saturating_sub(remaining_raw),
        };
        if expanded > available {
            if complete {
                return (Vec::new(), true);
            }
            used_text = used_text.saturating_add(raw.len());
            lines.push(Line {
                at_ms: None,
                text: raw.to_owned(),
            });
            continue;
        }
        used_text = used_text.saturating_add(expanded);
        for at in times {
            lines.push(Line {
                at_ms: Some(at.saturating_add(offset_ms).max(0) as u64),
                text: text.to_owned(),
            });
        }
    }
    while lines
        .last()
        .is_some_and(|line| line.at_ms.is_none() && line.text.is_empty())
    {
        lines.pop();
    }
    (lines, false)
}

/// The value of an `.lrc` header such as `[offset:+250]`, if this is one.
fn header<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let inner = line.trim().strip_prefix('[')?.strip_suffix(']')?;
    let (key, value) = inner.split_once(':')?;
    (key.trim().eq_ignore_ascii_case(name)).then_some(value)
}

/// `true` for the metadata lines of an `.lrc`: `[ar:…]`, `[ti:…]`, `[length:…]`.
fn is_header(line: &str) -> bool {
    let Some(inner) = line
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    else {
        return false;
    };
    let Some((key, _)) = inner.split_once(':') else {
        return false;
    };
    // A timestamp is `[mm:ss…]`, so a key of digits is not a header.
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic())
}

/// Every timestamp at the head of a line, and what follows them.
fn timestamps(line: &str) -> (Vec<i64>, &str) {
    let mut rest = line;
    let mut found = Vec::new();
    loop {
        let trimmed = rest.trim_start();
        let Some(close) = trimmed.find(']') else {
            break;
        };
        if !trimmed.starts_with('[') {
            break;
        }
        let Some(at) = moment(&trimmed[1..close]) else {
            break;
        };
        found.push(at);
        rest = &trimmed[close + 1..];
    }
    (found, rest)
}

/// `mm:ss`, `mm:ss.cc` or `mm:ss.mmm`, in milliseconds.
fn moment(inner: &str) -> Option<i64> {
    let (minutes, rest) = inner.split_once(':')?;
    let minutes: i64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once(['.', ':']) {
        Some((seconds, fraction)) => (seconds, fraction),
        None => (rest, ""),
    };
    let seconds: i64 = seconds.trim().parse().ok()?;
    // Two digits are hundredths, three are milliseconds — both are written.
    let digits: String = fraction
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let fraction: i64 = match digits.len() {
        0 => 0,
        1 => digits.parse::<i64>().ok()? * 100,
        2 => digits.parse::<i64>().ok()? * 10,
        _ => digits[..3].parse().ok()?,
    };
    minutes
        .checked_mul(60_000)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(fraction)
}

#[cfg(test)]
#[path = "lyrics_tests.rs"]
mod tests;
