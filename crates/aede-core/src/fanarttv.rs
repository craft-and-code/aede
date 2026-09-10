//! Fanart.tv: a portrait of the artist, when Wikidata has none, and its logo
//! and banner, neither of which Wikidata ever carries at all.
//!
//! Deliberately **no network**, like [`crate::acoustid`] and
//! [`crate::musicbrainz`]: this module builds the address to ask and reads
//! the answer somebody else fetched.
//!
//! # Second for a portrait, only, for a logo
//!
//! [`crate::wikipedia::portrait_file`] is asked before this for a portrait —
//! a Commons image is reached through the identifier MusicBrainz already
//! gave, and Commons hosts nothing whose licence is not stated. Fanart.tv is
//! user-submitted: `fetch --portraits` uses it only where Wikidata has
//! nothing, and the record it leaves says which service actually answered, so
//! a reader who cares about that distinction can tell — see [`SOURCE`].
//!
//! A logo has no such alternative to try first: Wikidata states no logo claim
//! for an artist, so `fetch --logos` asks Fanart.tv directly, under its own
//! record name — see [`LOGO_SOURCE`]. A banner rides the very same request:
//! `fetch --logos --banners` reads [`banner_url`] out of the answer already
//! fetched for the logo, rather than asking twice for one artist.
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
pub const WEB_SERVICE: &str = "https://webservice.fanart.tv/v3.2/music";

/// The name a logo is stored under — not [`SOURCE`], though the same request
/// answers both.
///
/// A portrait and a logo are two different claims about the same artist,
/// fetched by `fetch --portraits` and `fetch --logos` on their own schedules,
/// and a record that carried both could not be re-fetched for one without
/// silently dropping the other — `Sources::set` replaces a record whole, it
/// does not merge fields into it. Keeping them as two records under two names
/// is what lets each pass redo its own work without undoing the other's — the
/// same reasoning, and the same fix, as [`crate::wikipedia::PORTRAIT_SOURCE`].
pub const LOGO_SOURCE: &str = "fanarttv-logo";

/// The separate source record for a record label's logo.
pub const LABEL_LOGO_SOURCE: &str = "fanarttv-label-logo";

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

/// Where to ask about one record label, by its MusicBrainz label identifier.
pub fn label_lookup_url(mbid: &str, key: &str) -> String {
    format!("{WEB_SERVICE}/labels/{mbid}?api_key={key}")
}

/// Refuses a response whose identity does not match the artist requested.
///
/// Fanart.tv v3.2 guarantees `mbid_id` on artist answers. Without this guard,
/// an error object or a changed response shape reads indistinguishably from
/// "this artist has no logo".
pub fn artist_response(response: &Json, mbid: &str) -> Result<(), String> {
    match response.field_str("mbid_id") {
        Some(found) if found == mbid => Ok(()),
        Some(found) => Err(format!(
            "Fanart.tv answered for MusicBrainz id {found}, not {mbid}"
        )),
        None => Err(
            "Fanart.tv returned no artist identifier; the response was not an artist answer"
                .to_string(),
        ),
    }
}

/// Refuses a response whose identity does not match the label requested.
pub fn label_response(response: &Json, mbid: &str) -> Result<(), String> {
    match response.field_str("id") {
        Some(found) if found == mbid => Ok(()),
        Some(found) => Err(format!(
            "Fanart.tv answered for MusicBrainz label id {found}, not {mbid}"
        )),
        None => Err(
            "Fanart.tv returned no label identifier; the response was not a label answer"
                .to_string(),
        ),
    }
}

/// One image the service knows of, and how well liked it is.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Thumb {
    url: String,
    likes: u32,
}

/// The most liked image under `field`, or `None` when it is empty or absent.
///
/// Sorted by `likes`, which is the only signal the service gives about which
/// of several submissions is the one people chose — the same role a score
/// plays for a search result, and unlike a search result these are not scored
/// against the question at all, only against each other.
fn best(response: &Json, field: &str) -> Option<Thumb> {
    let mut found: Vec<Thumb> = response
        .get(field)
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

/// The most liked portrait the service holds for this artist, or `None`.
///
/// **`artistthumb` only.** Fanart.tv also serves banners, logos and backdrops
/// for the same artist, and those answer a different question — the wallpaper
/// behind a "now playing" screen, not "who is this". Bringing them in under
/// one function would ask a caller wanting a portrait to filter what should
/// never have been mixed together.
fn best_thumb(response: &Json) -> Option<Thumb> {
    best(response, "artistthumb")
}

/// The image address to download, or `None` when the service has nothing.
pub fn portrait_url(response: &Json) -> Option<String> {
    best_thumb(response).map(|thumb| thumb.url)
}

/// The artist's logo address to download, or `None` when the service has
/// none.
///
/// **`hdmusiclogo`, then `musiclogo`.** The two are the same submissions at
/// two resolutions, not two different pools to rank against each other — an
/// HD entry always wins over a standard one, however many likes the standard
/// one has, and `musiclogo` is only consulted when `hdmusiclogo` is empty.
pub fn logo_url(response: &Json) -> Option<String> {
    best(response, "hdmusiclogo")
        .or_else(|| best(response, "musiclogo"))
        .map(|thumb| thumb.url)
}

/// The artist's banner address to download, or `None` when the service has
/// none.
///
/// **`musicbanner`.** Unlike the logo, the service serves this at one
/// resolution only — there is no `hdmusicbanner` to prefer first.
pub fn banner_url(response: &Json) -> Option<String> {
    best(response, "musicbanner").map(|thumb| thumb.url)
}

/// The record label's logo address, or `None` when it has none.
pub fn label_logo_url(response: &Json) -> Option<String> {
    best(response, "musiclabel").map(|thumb| thumb.url)
}

/// What to tell somebody who has no application key.
///
/// Named separately from [`crate::acoustid::no_key`] rather than shared: the
/// two services, the two registration pages and the two variables are not the
/// same fact stated twice, and a reader following one must not be sent to
/// register at the other's address by an error message that forgot which it
/// was talking about.
///
/// States only the key itself, never who is affected: `--portraits` can fall
/// back to Wikidata and `--logos` cannot, so which artists a missing key
/// leaves untouched is a different sentence for each — one this function does
/// not know and must not guess at — and each pass says it in the line before
/// this one is printed.
pub fn no_key() -> String {
    format!(
        "\
Fanart.tv images need an application key, and none is set.

  1. Register at https://fanart.tv/get-an-api-key/
  2. export {KEY_VARIABLE}=<the key it gives you>"
    )
}

#[cfg(test)]
#[path = "fanarttv_tests.rs"]
mod tests;
