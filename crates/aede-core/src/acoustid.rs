//! AcoustID: what a recording is, asked of its sound.
//!
//! Deliberately **no network**, like [`crate::musicbrainz`] and
//! [`crate::coverart`]: this module builds the address to ask and reads the
//! answer somebody else fetched. Reaching it is the client's job.
//!
//! # What it answers, and what it cannot
//!
//! Shown a fingerprint and a length, AcoustID answers with **recordings** —
//! MusicBrainz recording identifiers, with a score saying how sure it is. A
//! recording is a performance, not a release: the same recording appears on an
//! album, a compilation and a deluxe reissue, so the answer identifies *what
//! is playing* and not *which pressing this file was ripped from*.
//!
//! That is the right answer for the question this feature exists to ask —
//! "what is this untagged file?" — and the wrong one for "which edition do I
//! own", which the tags answer better than any fingerprint could.
//!
//! # The score is kept, and it is not a certainty
//!
//! Everything here arrives as [`crate::sources::Confidence::Matched`], never
//! `Identified`. A fingerprint match is a strong guess about a performance and
//! is wrong in ways that are easy to picture: two masterings of one recording
//! fingerprint alike, a live version and its studio original sometimes do, and
//! a silent or very short track matches a great deal. The score comes back
//! with the record so that a reader can see what they are being told.
//!
//! # It identifies; it never corrects
//!
//! Nothing here writes a tag, because nothing in Aède writes into an audio
//! file. What the service says is filed **beside** the tags, in the attributed
//! layer, and shown next to them — so a file whose audio and tags disagree is
//! reported rather than quietly rewritten. Retagging is the user's tagger's
//! job, and this is not one.

use crate::json::Json;

/// The name records from this service carry in `sources.json`.
pub const SOURCE: &str = "acoustid";

/// Base address of the lookup endpoint.
pub const WEB_SERVICE: &str = "https://api.acoustid.org/v2/lookup";

/// The environment variable holding the application key.
///
/// AcoustID requires one and Aède ships none, for a reason worth stating:
/// a key baked into an open-source binary is a key every copy of the program
/// shares, and the quota with it. Registering takes a minute and the key is
/// free — [`no_key`] says where.
pub const KEY_VARIABLE: &str = "AEDE_ACOUSTID_KEY";

/// The wait between two requests.
///
/// The service asks for no more than three per second. Aède sends one every
/// 350 ms — under the limit with room for a clock that rounds the wrong way,
/// the same margin [`crate::musicbrainz::REQUEST_INTERVAL`] leaves.
pub const REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(350);

/// What the answer must contain to be worth parsing.
///
/// Without `meta` the service answers with its own track identifiers and
/// nothing else — true, useless, and indistinguishable at a glance from a
/// full answer. `recordings` brings the MusicBrainz identifier, the title and
/// the artists; `releasegroups` brings the album, which is what makes the
/// answer readable next to a tag.
const META: &str = "recordings+releasegroups";

/// Where to ask about one fingerprint.
///
/// The fingerprint is base64 and carries `+` and `/`, both of which mean
/// something else in a query string, so it is escaped — a fingerprint sent raw
/// comes back as "not found" for a file the service knows perfectly well.
pub fn lookup_url(key: &str, fingerprint: &str, seconds: u32) -> String {
    format!(
        "{WEB_SERVICE}?client={}&meta={META}&duration={seconds}&fingerprint={}",
        escape(key),
        escape(fingerprint)
    )
}

/// Percent-encodes what a query string cannot carry literally.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// What the service says one file is.
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    /// The MusicBrainz **recording** identifier — a performance, not a release.
    pub recording: String,
    /// How sure the service is, from 0 to 1, as it stated it.
    pub score: f64,
    /// The recording's title, when the answer carries one.
    pub title: Option<String>,
    /// The artists credited, in the order given.
    pub artists: Vec<String>,
    /// A release group the recording appears on, when one is named.
    ///
    /// One of possibly many, and it is not "the album this file came from":
    /// a recording sits on the original, the compilation and the reissue
    /// alike. It is here to make the answer readable, not to be believed
    /// over a tag.
    pub album: Option<String>,
}

/// The best thing the service said, or nothing.
///
/// **Highest score wins, and a result with no recording is not an answer.**
/// The service replies with its own track identifiers even when it knows
/// nothing else about them, and a caller that took the first result would file
/// a record naming a track nobody can look up.
///
/// Ties are broken by the recording identifier so that two runs over an
/// unchanged file answer the same thing — a match that changes between runs
/// cannot be compared with the one stored last time.
pub fn best(response: &Json) -> Option<Heard> {
    let mut found: Vec<Heard> = Vec::new();
    for result in response.get("results").and_then(Json::as_arr)? {
        let score = result.field_f64("score").unwrap_or(0.0);
        let Some(recordings) = result.get("recordings").and_then(Json::as_arr) else {
            continue;
        };
        for recording in recordings {
            let Some(id) = recording.field_str("id").filter(|id| !id.is_empty()) else {
                continue;
            };
            found.push(Heard {
                recording: id,
                score,
                title: recording.field_str("title").filter(|t| !t.is_empty()),
                artists: recording
                    .get("artists")
                    .and_then(Json::as_arr)
                    .map(|list| list.iter().filter_map(|a| a.field_str("name")).collect())
                    .unwrap_or_default(),
                album: recording
                    .get("releasegroups")
                    .and_then(Json::as_arr)
                    .and_then(|list| list.first())
                    .and_then(|group| group.field_str("title")),
            });
        }
    }
    found.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.recording.cmp(&b.recording))
    });
    found.into_iter().next()
}

/// `true` when the service reported a failure rather than an empty answer.
///
/// `status` is `ok` or `error`, and an error carries a message worth showing —
/// a bad key answers `200 OK` with `"status": "error"`, which a client
/// reading only the HTTP code would file as "this file is unknown" for every
/// file in the library.
pub fn refused(response: &Json) -> Option<String> {
    match response.field_str("status").as_deref() {
        Some("error") | None => Some(
            response
                .get("error")
                .and_then(|e| e.field_str("message"))
                .unwrap_or_else(|| "the service did not say it was ok".to_string()),
        ),
        _ => None,
    }
}

/// What to tell somebody who has no application key.
///
/// The address is given, and so is the reason there is no key in the program:
/// a reader who is told to "set AEDE_ACOUSTID_KEY" and nothing else has been
/// handed a chore with no explanation.
pub fn no_key() -> String {
    format!(
        "\
Identifying by sound needs an AcoustID application key, and none is set.

AcoustID is free and asks every program using it to register, so that a
misbehaving one can be stopped without stopping everyone. Aède ships no key
of its own on purpose: a key inside an open-source program is a key every
copy shares, and the quota with it.

  1. Register at https://acoustid.org/new-application
  2. export {KEY_VARIABLE}=<the key it gives you>

Nothing else in Aède needs it."
    )
}

#[cfg(test)]
#[path = "acoustid_tests.rs"]
mod tests;
