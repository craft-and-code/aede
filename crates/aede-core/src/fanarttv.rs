//! Fanart.tv: a portrait of the artist, when Wikidata has none.
//!
//! Deliberately **no network**, like [`crate::acoustid`] and
//! [`crate::musicbrainz`]: this module builds the address to ask and reads
//! the answer somebody else fetched.
//!
//! # Second, never first
//!
//! [`crate::wikipedia::portrait_file`] is asked before this — a Commons image
//! is reached through the identifier MusicBrainz already gave, and Commons
//! hosts nothing whose licence is not stated. Fanart.tv is user-submitted:
//! `fetch --portraits` uses it only where Wikidata has nothing, and the record
//! it leaves says which service actually answered, so a reader who cares about
//! that distinction can tell — see [`SOURCE`].
//!
//! # Indexed by the identifier that never needed a search
//!
//! Unlike AcoustID, which is shown a fingerprint and has to guess which
//! recording it is, Fanart.tv is asked about an artist **by MusicBrainz
//! identifier** — the same one every artist record in `sources.json` already
//! carries. There is nothing to score and nothing to arbitrate: the answer is
//! either this artist's images or none.

use crate::json::Json;

/// The name records from this service carry in `sources.json`.
pub const SOURCE: &str = "fanarttv";

/// Base address of the service.
pub const WEB_SERVICE: &str = "https://webservice.fanart.tv/v3/music";

/// The environment variable holding the application key.
///
/// Free to register, and not shipped with Aède for the reason
/// [`crate::acoustid::KEY_VARIABLE`] gives: a key baked into an open-source
/// binary is a key every copy shares, and the quota with it.
pub const KEY_VARIABLE: &str = "AEDE_FANARTTV_KEY";

/// The key this machine holds, when it holds one.
///
/// Read once, at the edge, and carried in — the same discipline
/// [`crate::acoustid::key`] follows, for the same reason: a value read from
/// the process environment inside the pass that needs it cannot be handed a
/// different answer by a test.
pub fn key() -> Option<String> {
    std::env::var(KEY_VARIABLE)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// How long to wait between requests.
///
/// Fanart.tv states no published limit; this keeps to the same one request a
/// second the rest of Aède treats as the polite default rather than assume a
/// service that has not asked for restraint wants none.
pub const REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1000);

/// Where to ask about one artist.
pub fn lookup_url(mbid: &str, key: &str) -> String {
    format!("{WEB_SERVICE}/{mbid}?api_key={key}")
}

/// A portrait the service knows of, and how well liked it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Thumb {
    url: String,
    likes: u32,
}

/// The most liked portrait the service holds for this artist, or `None`.
///
/// **`artistthumb` only.** Fanart.tv also serves banners, logos and backdrops
/// for the same artist, and those answer a different question — the wallpaper
/// behind a "now playing" screen, not "who is this". Bringing them in under
/// one function would ask a caller wanting a portrait to filter what should
/// never have been mixed together.
///
/// Sorted by `likes`, which is the only signal the service gives about which
/// of several submissions is the one people chose — the same role a score
/// plays for a search result, and unlike a search result these are not scored
/// against the question at all, only against each other.
fn best_thumb(response: &Json) -> Option<Thumb> {
    let mut found: Vec<Thumb> = response
        .get("artistthumb")
        .and_then(Json::as_arr)
        .unwrap_or(&[])
        .iter()
        .filter_map(|row| {
            let url = row.field_str("url").filter(|u| !u.is_empty())?;
            let likes = row
                .field_str("likes")
                .and_then(|l| l.parse::<u32>().ok())
                .unwrap_or(0);
            Some(Thumb { url, likes })
        })
        .collect();
    found.sort_by(|a, b| b.likes.cmp(&a.likes).then_with(|| a.url.cmp(&b.url)));
    found.into_iter().next()
}

/// The image address to download, or `None` when the service has nothing.
pub fn portrait_url(response: &Json) -> Option<String> {
    best_thumb(response).map(|thumb| thumb.url)
}

/// What to tell somebody who has no application key.
///
/// Named separately from [`crate::acoustid::no_key`] rather than shared: the
/// two services, the two registration pages and the two variables are not the
/// same fact stated twice, and a reader following one must not be sent to
/// register at the other's address by an error message that forgot which it
/// was talking about.
pub fn no_key() -> String {
    format!(
        "\
Fanart.tv images need an application key, and none is set. This only affects
artists Wikidata has no portrait for — the rest are unaffected.

  1. Register at https://fanart.tv/get-an-api-key/
  2. export {KEY_VARIABLE}=<the key it gives you>"
    )
}

#[cfg(test)]
#[path = "fanarttv_tests.rs"]
mod tests;
