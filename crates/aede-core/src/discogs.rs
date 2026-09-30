//! Discogs label profiles reached through a MusicBrainz URL relationship.
//!
//! This module parses identifiers and responses without making requests. The
//! CLI decides when a reader explicitly asks to go online.

use crate::json::Json;
use crate::sources::Prose;
use crate::text;
use std::collections::{BTreeMap, BTreeSet};

/// Source name in the attributed layer.
pub const SOURCE: &str = "discogs";
/// At most 24 requests per minute, below the unauthenticated limit of 25.
pub const REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(2500);

/// Keep a margin below the API terms' six-hour display limit.
pub const MAX_AGE_SECONDS: u64 = 5 * 60 * 60;

/// Whether a stored answer is young enough to display under the API terms.
pub fn fresh(fetched_at: u64, now: u64) -> bool {
    fetched_at != 0 && now >= fetched_at && now - fetched_at < MAX_AGE_SECONDS
}

/// Accept only a Discogs label URL and use its numeric identifier on our own
/// fixed API host. No URL found in external metadata is fetched directly.
pub fn label_id(url: &str) -> Option<u64> {
    let path = ["https://www.discogs.com/", "https://discogs.com/"]
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let path = path.strip_prefix("fr/").unwrap_or(path);
    let label = path.strip_prefix("label/")?;
    let digits: String = label.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty()
        || !matches!(
            label.as_bytes().get(digits.len()).copied(),
            None | Some(b'-' | b'/' | b'?' | b'#')
        )
    {
        return None;
    }
    digits.parse::<u64>().ok().filter(|id| *id > 0)
}

/// Public page to cite beside a Discogs profile.
pub fn label_url(id: u64) -> String {
    format!("https://www.discogs.com/label/{id}")
}

/// Fixed API endpoint for a label ID already checked against a Discogs URL.
pub fn api_url(id: u64) -> String {
    format!("https://api.discogs.com/labels/{id}")
}

/// Fixed API endpoint for an artist named in a label profile.
pub fn artist_api_url(id: u64) -> String {
    format!("https://api.discogs.com/artists/{id}")
}

/// A label's profile is CC0 data under Discogs' API terms. The display also
/// prints the required “Data provided by Discogs.” notice beside the text.
pub fn profile(response: &Json, id: u64, expected_name: &str) -> Option<Prose> {
    let found_id = response.field_u32("id")? as u64;
    let found_name = response.field_str("name")?;
    if found_id != id || text::normalize(&found_name) != text::normalize(expected_name) {
        return None;
    }
    let content = response.field_str("profile")?;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    Some(Prose {
        text: content.to_string(),
        url: label_url(id),
        // Discogs profiles are user-written, and the API does not state the
        // language of each one. “und” avoids claiming it is English.
        lang: "und".to_string(),
        licence: "CC0".to_string(),
    })
}

/// IDs named by Discogs' `[l123]` label-link markup, in first-use order.
pub fn referenced_labels(raw: &str) -> Vec<u64> {
    referenced_ids(raw, 'l')
}

/// Artist IDs named by Discogs' `[a123]` markup, in first-use order.
pub fn referenced_artists(raw: &str) -> Vec<u64> {
    referenced_ids(raw, 'a')
}

fn referenced_ids(raw: &str, kind: char) -> Vec<u64> {
    let mut ids = Vec::new();
    let mut seen = BTreeSet::new();
    let mut rest = raw;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        if let Some(id) = reference_id(&after[..end], kind)
            && seen.insert(id)
        {
            ids.push(id);
        }
        rest = &after[end + 1..];
    }
    ids
}

/// Plain-text presentation of the Discogs profile. Unknown and malformed
/// markup is retained so a source's ordinary bracketed text is not lost.
pub fn render_profile(
    raw: &str,
    labels: &BTreeMap<u64, String>,
    artists: &BTreeMap<u64, String>,
) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &after[..end];
        if let Some(id) = reference_id(token, 'l') {
            match labels.get(&id) {
                Some(name) => out.push_str(name),
                None => out.push_str(&format!("Label #{id}")),
            }
        } else if let Some(id) = reference_id(token, 'a') {
            match artists.get(&id) {
                Some(name) => out.push_str(name),
                None => out.push_str(&format!("Artist #{id}")),
            }
        } else if let Some(name) = token.strip_prefix("l=") {
            out.push_str(name);
        } else if let Some(name) = token.strip_prefix("a=") {
            out.push_str(name);
        } else if !matches!(token, "b" | "/b" | "i" | "/i" | "u" | "/u" | "s" | "/s") {
            out.push('[');
            out.push_str(token);
            out.push(']');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The name from a second label lookup must belong to the requested ID.
pub fn label_name(response: &Json, id: u64) -> Option<String> {
    named_entity(response, id)
}

/// The name from an artist lookup must belong to the requested ID.
pub fn artist_name(response: &Json, id: u64) -> Option<String> {
    named_entity(response, id)
}

fn named_entity(response: &Json, id: u64) -> Option<String> {
    if response.field_u32("id")? as u64 != id {
        return None;
    }
    response
        .field_str("name")
        .filter(|name| !name.trim().is_empty())
}

fn reference_id(token: &str, kind: char) -> Option<u64> {
    let digits = token.strip_prefix(kind)?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u64>().ok().filter(|id| *id > 0)
}

#[cfg(test)]
#[path = "discogs_tests.rs"]
mod tests;
