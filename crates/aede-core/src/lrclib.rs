//! Asking LRCLIB for the words, and reading what it answers.
//!
//! No socket here, exactly as in [`crate::musicbrainz`]: this builds addresses
//! and reads answers, and everything that waits its turn lives in
//! [`crate::http`]. That is what makes the whole of it testable against canned
//! text.
//!
//! # Why this service and not another
//!
//! Lyrics are the **composition's** copyright, and owning a FLAC grants no
//! rights in it — they are legally a different object from the recording. Of
//! the free sources, LRCLIB is the only one an open-source program can query
//! without breaking an API's own terms: Genius forbids the scraping its lyrics
//! require, and Musixmatch's free tier is non-commercial and truncated. The
//! reasoning in full is in `docs/design/lyrics.md`.
//!
//! It is also the gentlest network path this program has. No key, no account, a
//! JSON document of a few fields, and a failure that costs nothing: no lyrics
//! is not a broken catalog, where a bad MusicBrainz match attaches the wrong
//! record to an album.
//!
//! # Identify yourself
//!
//! LRCLIB drops connections from agents it has come to distrust — Jellyfin's
//! server agent is refused outright, with no status code at all, the connection
//! simply closing. This program sends the same honest `User-Agent` it sends
//! MusicBrainz, and shares its throttle: being slower than a free service
//! requires is never the wrong mistake.

use crate::json::Json;

/// Where the service lives.
pub const WEB_SERVICE: &str = "https://lrclib.net/api";

/// What the service had for one track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// Words, timed or not.
    Words(Words),
    /// The service knows this recording and says it has no words at all.
    ///
    /// **Not the same as not knowing**, and the distinction is the reason this
    /// is a variant rather than an empty [`Words`]: an instrumental is a
    /// finished question, and telling a reader "no lyrics found" about one
    /// would send them looking for a fault that is not there.
    Instrumental,
    /// The service has nothing for this track.
    Nothing,
}

/// The words themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    /// The text, in the form it will be written to disk.
    pub text: String,
    /// Whether the text carries timings.
    ///
    /// The service holds both forms and they are two different things to a
    /// reader: one is a page to read, the other is a file a player can follow.
    /// The synced form is preferred where there is one, and this says which was
    /// taken so the run can report it rather than leave it to be discovered.
    pub synced: bool,
}

/// The address that asks about one track.
///
/// **Four criteria, and only two of them are the service's requirement.** A
/// live answer to `artist_name` and `track_name` alone came back `200`, so the
/// album and the length are *this program's* insistence rather than LRCLIB's:
/// a song has a studio take, a live one and three covers, they share a title,
/// and the length is what tells them apart. Both are things this catalog holds
/// exactly — the duration is measured on the stream rather than read from a
/// tag — so asking the narrow question costs nothing and answers about this
/// recording.
///
/// An empty album is **left out of the address** rather than sent empty. The
/// two are different questions: an omitted parameter asks with one criterion
/// fewer, where `album_name=` states that the album is called nothing. Only the
/// first has been seen to work, and inventing the behaviour of the second is
/// exactly what a checked answer is for.
pub fn get_url(artist: &str, title: &str, album: &str, duration_secs: u64) -> String {
    let mut url = format!(
        "{WEB_SERVICE}/get?artist_name={}&track_name={}",
        encode(artist),
        encode(title)
    );
    if !album.trim().is_empty() {
        url.push_str(&format!("&album_name={}", encode(album)));
    }
    url.push_str(&format!("&duration={duration_secs}"));
    url
}

/// Percent-encodes a query value.
///
/// Local to this module rather than shared with the MusicBrainz client: that
/// one escapes a *search query* first, in a syntax with its own reserved
/// characters, and folding the two would mean one caller quietly getting the
/// other's rules.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Reads what the service answered.
///
/// The synced form wins where there is one, because it is the richer of the
/// two: [`crate::lyrics::parse`] reads it into lines that carry their timings,
/// and a reader who only wants the words gets them from the same lines with the
/// timings dropped. Taking the plain form when a timed one exists would throw
/// away something that cannot be recovered.
///
/// An answer whose text is empty is [`Found::Nothing`], whatever else it says:
/// a record with no words in it is not an answer about the words.
pub fn read(value: &Json) -> Found {
    // Asked before the text, because it is an answer *about* the absence of
    // text and would otherwise be read as a service that failed to have any.
    if value.field_optional_bool("instrumental") == Some(true) {
        return Found::Instrumental;
    }
    let synced = value
        .field_str("syncedLyrics")
        .filter(|t| !t.trim().is_empty());
    let plain = value
        .field_str("plainLyrics")
        .filter(|t| !t.trim().is_empty());
    match (synced, plain) {
        (Some(text), _) => Found::Words(Words { text, synced: true }),
        (None, Some(text)) => Found::Words(Words {
            text,
            synced: false,
        }),
        (None, None) => Found::Nothing,
    }
}

#[cfg(test)]
#[path = "lrclib_tests.rs"]
mod tests;
