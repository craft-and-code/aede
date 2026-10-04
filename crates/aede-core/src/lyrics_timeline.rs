//! Shared lookup for clients following lyrics from their own playback clock.

use super::Lyrics;

/// All lyric lines beginning at one moment in a track.
///
/// A cue remains active until the next cue, or through the end of the track.
/// A cue containing only empty text deliberately clears the previous words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    /// Milliseconds from the beginning of the decoded track.
    ///
    /// This is the same time base as [`super::Line::at_ms`]. Seeking retains
    /// that origin; changing the output sample rate does not restart it. The
    /// parser has already applied any LRC offset, so clients must not apply it
    /// a second time.
    pub at_ms: u64,
    /// Zero-based indices into the original [`Lyrics::lines`], in source order.
    ///
    /// Equal timestamps share a cue, including repeated timestamps and blank
    /// lines. The referenced lyrics must remain unchanged while these indices
    /// are used; rebuild the timeline after editing or replacing them.
    pub line_indices: Vec<usize>,
}

/// A chronological index of a track's synchronized lyrics.
///
/// Preparation takes O(n log n) time and O(n) auxiliary storage for n lyric
/// lines, without cloning their text. Each position lookup takes O(log c)
/// time for c distinct cues. There is no clock or moving cursor: a caller can
/// pause, seek backward or repeat by supplying its current track position.
///
/// Untimed lines remain in [`Lyrics`] for plain-text display, but are excluded
/// here: the index never invents a timing for metadata or unmarked verses.
/// Timed blank lines are retained because they stop the previous text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Timeline {
    cues: Vec<Cue>,
}

impl Timeline {
    /// Indexes all timed lines without changing their source order or content.
    ///
    /// Input timestamps may be unsorted. Chronological order is deterministic,
    /// with original line order breaking ties. The index owns only timestamps
    /// and indices, so the caller keeps the unmodified [`Lyrics`] alongside it.
    /// Memory grows linearly with the supplied lines; this constructor does
    /// not impose another size limit on publicly constructed lyrics.
    pub fn new(lyrics: &Lyrics) -> Self {
        let mut timed: Vec<_> = lyrics
            .lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.at_ms.map(|at_ms| (at_ms, index)))
            .collect();
        timed.sort_unstable();

        let mut cues: Vec<Cue> = Vec::new();
        for (at_ms, index) in timed {
            match cues.last_mut() {
                Some(cue) if cue.at_ms == at_ms => cue.line_indices.push(index),
                _ => cues.push(Cue {
                    at_ms,
                    line_indices: vec![index],
                }),
            }
        }
        Self { cues }
    }

    /// The distinct cues in increasing timestamp order.
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    /// Whether the lyrics have no timed cue.
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// Finds the most recent cue at or before a decoded-track position.
    ///
    /// The timestamp is inclusive. Before the first cue, or with plain lyrics,
    /// this returns `None`. The last cue remains active until the caller ends
    /// the track; no lyric duration is guessed from its text. An active blank
    /// cue is returned rather than skipped, allowing the client to clear text.
    pub fn active_at(&self, position_ms: u64) -> Option<&Cue> {
        let end = self.cues.partition_point(|cue| cue.at_ms <= position_ms);
        end.checked_sub(1).and_then(|index| self.cues.get(index))
    }
}

#[cfg(test)]
#[path = "lyrics_timeline_tests.rs"]
mod tests;
