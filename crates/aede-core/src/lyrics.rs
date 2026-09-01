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

use std::path::{Path, PathBuf};

/// Extension of a lyrics file beside a track.
pub const EXTENSION: &str = "lrc";

/// At most this much of a lyrics file is read.
///
/// Lyrics are a page of text. A ten-megabyte `.lrc` is not a song, and reading
/// it whole to show a track page would be paying for somebody's mistake.
const LIMIT: usize = 256 * 1024;

/// One line, with the moment it is sung when the file says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Milliseconds from the start of the track, or `None` on a plain line.
    ///
    /// Kept at read time rather than recovered later: M3 needs a playhead to
    /// follow, and asking it to re-read every file would be asking twice for
    /// something already in hand.
    pub at_ms: Option<u64>,
    /// The line as written, without its timestamp.
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
/// because of one accent would be the wrong trade.
pub fn read(path: &Path) -> Option<Lyrics> {
    let raw = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&raw[..raw.len().min(LIMIT)]).to_string();
    let lines = parse(&text);
    match lines.is_empty() {
        true => None,
        false => Some(Lyrics {
            source: Source::Sidecar,
            origin: path.to_string_lossy().to_string(),
            lines,
        }),
    }
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
/// a file that was timed against another encoding.
pub fn parse(text: &str) -> Vec<Line> {
    let mut offset_ms: i64 = 0;
    let mut lines = Vec::new();
    for raw in text.lines() {
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
                lines.push(Line { at_ms: None, text });
            }
            continue;
        }
        // `[00:12.00][01:44.00] the chorus again` is one line sung twice, and
        // the file means both: a chorus that only appeared once would leave a
        // player silent at its second turn.
        for at in times {
            lines.push(Line {
                at_ms: Some(at.saturating_add(offset_ms).max(0) as u64),
                text: rest.trim().to_string(),
            });
        }
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    lines
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
    Some(minutes * 60_000 + seconds * 1_000 + fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_text_file_is_lines_with_no_times() {
        let lines = parse("First line\nSecond line\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            Line {
                at_ms: None,
                text: "First line".into()
            }
        );
        assert!(lines.iter().all(|l| l.at_ms.is_none()));
    }

    #[test]
    fn a_timed_line_gives_up_its_moment() {
        // The three spellings that are actually written: seconds, hundredths,
        // milliseconds.
        let lines = parse("[00:12]a\n[00:12.34]b\n[01:02.345]c\n");
        assert_eq!(
            lines.iter().map(|l| l.at_ms).collect::<Vec<_>>(),
            vec![Some(12_000), Some(12_340), Some(62_345)]
        );
        assert_eq!(lines[2].text, "c");
    }

    #[test]
    fn a_chorus_timed_twice_appears_twice() {
        // `[00:12][01:44] the chorus` is one line sung twice, and a reader that
        // kept only the first would leave a player silent at its second turn.
        let lines = parse("[00:12.00][01:44.00]the chorus\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].at_ms, Some(12_000));
        assert_eq!(lines[1].at_ms, Some(104_000));
        assert_eq!(lines[0].text, lines[1].text);
    }

    #[test]
    fn the_metadata_headers_are_dropped_and_the_offset_applied() {
        // `[ar:]` and friends repeat what the tags already say, and this file is
        // not where the artist's name is settled. `[offset:]` is different: it
        // exists to shift a timing that was made against another encoding.
        let lines = parse("[ar:Ozzy]\n[ti:Crazy Train]\n[offset:+500]\n[00:10.00]all aboard\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].at_ms, Some(10_500));
        let back = parse("[offset:-2000]\n[00:10.00]all aboard\n");
        assert_eq!(back[0].at_ms, Some(8_000));
        // An offset that would send a line before the start clamps there rather
        // than wrapping around into an enormous number.
        let far = parse("[offset:-99000]\n[00:10.00]all aboard\n");
        assert_eq!(far[0].at_ms, Some(0));
    }

    #[test]
    fn a_tag_holding_lrc_is_read_as_lrc() {
        // Plenty of taggers write the synced text straight into `LYRICS`, and a
        // reader that only understood plain text would show a page of
        // `[00:12.34]` to somebody who asked for the words.
        let lyrics = from_tag("/m/a.flac", "[00:01.00]one\n[00:02.00]two").expect("lyrics");
        assert!(lyrics.synced());
        assert_eq!(lyrics.text(), "one\ntwo");
        assert_eq!(lyrics.source, Source::Tag);
    }

    #[test]
    fn a_verse_break_survives_and_the_edges_do_not() {
        let lyrics = from_tag("/m/a.flac", "\n\nfirst\n\nsecond\n\n\n").expect("lyrics");
        assert_eq!(lyrics.text(), "first\n\nsecond");
        assert!(!lyrics.synced());
    }

    #[test]
    fn nothing_at_all_is_nothing_rather_than_empty_lyrics() {
        // A tag holding spaces is a tag holding nothing, and a page announcing
        // "Lyrics" over a blank space is worse than a page with no such
        // section.
        assert!(from_tag("/m/a.flac", "   \n\n  ").is_none());
        assert!(from_tag("/m/a.flac", "").is_none());
    }

    #[test]
    fn a_sidecar_sits_beside_its_track_under_the_same_name() {
        assert_eq!(
            sidecar_of(Path::new("/m/Album/01 So What.flac")),
            Path::new("/m/Album/01 So What.lrc")
        );
        assert!(is_sidecar("01 So What.lrc"));
        assert!(is_sidecar("01 So What.LRC"));
        assert!(!is_sidecar("01 So What.flac"));
        assert!(!is_sidecar("lrc"));
    }

    #[test]
    fn a_sidecar_is_read_bounded_and_a_missing_one_is_no_error() {
        let dir = std::env::temp_dir().join("aede_lyrics_read");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(read(&dir.join("gone.lrc")).is_none());

        let path = dir.join("song.lrc");
        std::fs::write(&path, "[00:01.00]one\n[00:02.00]two\n").unwrap();
        let lyrics = read(&path).expect("lyrics");
        assert_eq!(lyrics.source, Source::Sidecar);
        assert_eq!(lyrics.origin, path.to_string_lossy());
        assert!(lyrics.synced());

        // Bytes that are not UTF-8 are replaced, not refused: a .lrc written on
        // a Windows machine in 2003 is Latin-1 as often as not.
        let latin = dir.join("latin.lrc");
        std::fs::write(&latin, [b'c', b'a', b'f', 0xE9]).unwrap();
        assert!(read(&latin).is_some());

        // And a file nobody should have called lyrics is not read whole.
        let huge = dir.join("huge.lrc");
        std::fs::write(&huge, "x\n".repeat(LIMIT)).unwrap();
        let lyrics = read(&huge).expect("lyrics");
        assert!(lyrics.text().len() <= LIMIT, "{}", lyrics.text().len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
