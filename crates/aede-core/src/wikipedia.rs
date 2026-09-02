//! Wikipedia, reached through Wikidata: the prose MusicBrainz does not hold.
//!
//! MusicBrainz answers with identifiers, dates and relationships. It never
//! answers with a sentence about the artist, because that is not what it is
//! for. The sentence exists on Wikipedia — and the way from one to the other
//! is the `wikidata` relationship MusicBrainz already returns.
//!
//! **Two requests, not one.** A Wikidata entity is the hub: it holds a
//! *sitelink* per language, and a sitelink is a page title rather than a
//! summary. So reaching a paragraph is
//!
//! ```text
//! MusicBrainz artist  →  wikidata: .../wiki/Q11649
//!                        Special:EntityData/Q11649.json   →  sitelinks.enwiki.title
//!                        en.wikipedia.org/…/summary/<title>  →  extract
//! ```
//!
//! That is why this is opt-in rather than part of every fetch: it doubles a
//! run that already takes ten minutes over a large library, and the summary
//! is the one fact here nobody needs in order to file their music.
//!
//! **Everything in this module is a decision, and none of it is a request.**
//! The two URL builders and the two extractors below are pure functions over
//! text and JSON, so what the program concludes from an answer is proven by
//! the tests beside them, and only the handing of a URL to a client library is
//! left unproven — the same split [`crate::musicbrainz`] follows.
//!
//! # The licence is not optional
//!
//! Wikipedia text is CC BY-SA. Reusing it obliges attribution, so the extract
//! is only ever produced as a [`Prose`], which cannot hold the words without
//! the page they came from and the terms they came under. There is deliberately
//! no function here that returns a bare `String` of article text.

use crate::json::Json;
use crate::sources::Prose;

/// The name this source is stored under in `sources.json`.
///
/// "wikipedia" rather than "wikidata": Wikidata is the road, the article is
/// what a reader is being shown, and the credit has to name the article.
pub const SOURCE: &str = "wikipedia";

/// The licence Wikipedia article text is under.
///
/// Named as Wikimedia names it, because a credit that renames a licence is not
/// a credit. Stored per record rather than assumed at display time: if this
/// ever changes, records fetched before the change still say what they were
/// taken under.
pub const LICENCE: &str = "CC BY-SA 4.0";

/// How long to leave between requests.
///
/// Wikimedia's limits are far looser than MusicBrainz's, but the polite rate
/// is the one that never needs revisiting, and a run that is already paced at
/// one per second gains nothing from going faster in one leg of it.
pub const REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1100);

/// Languages to look for an article in, best first.
///
/// English last rather than first would be the friendlier default and the
/// wrong one: this list is a fallback chain, and the caller puts the reader's
/// own language at its head. English is here as the floor, because for a great
/// many artists it is the only article that exists.
pub const FALLBACK_LANGS: [&str; 1] = ["en"];

/// The Wikidata entity id inside a URL MusicBrainz returned.
///
/// MusicBrainz gives the relationship as a page URL rather than a bare id, and
/// it does not always give the same one: `www.wikidata.org/wiki/Q11649` is the
/// usual shape, but the entity URI `.../entity/Q11649` appears too. Both end
/// in the id, so the id is read from the end rather than the middle.
///
/// Returns `None` for anything that does not end in an entity id, which is the
/// honest answer for a link that leads somewhere else — a lexeme, a property,
/// or a URL that was never Wikidata at all.
pub fn entity_id(url: &str) -> Option<String> {
    let last = url.trim_end_matches('/').rsplit('/').next()?;
    // An entity id is `Q` and at least one digit, all digits. Checking the
    // shape is what stops `.../wiki/Special:EntityData` or a property `P31`
    // from being sent out as a lookup that cannot succeed.
    let digits = last.strip_prefix('Q')?;
    match !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        true => Some(last.to_string()),
        false => None,
    }
}

/// Where to ask Wikidata about an entity.
///
/// `Special:EntityData` rather than the `wbgetentities` API: it is a plain
/// document at a stable address, it needs no parameters to get wrong, and it
/// is cached, which is the difference between being a polite client and being
/// a load.
pub fn entity_data_url(id: &str) -> String {
    format!("https://www.wikidata.org/wiki/Special:EntityData/{id}.json")
}

/// An article on one language's Wikipedia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// The language code — `en`, `fr` — which is also part of the address.
    pub lang: String,
    /// The page title, as Wikidata spells it, spaces and all.
    pub title: String,
}

/// The article to read, from a `Special:EntityData` document.
///
/// `preferred` is tried in order, so a caller can ask for the reader's own
/// language and fall back rather than choosing between "always English" and
/// "nothing".
///
/// The document is keyed by entity id at the top, and an `EntityData` response
/// may carry redirects — asking for `Q1` and being answered about `Q2` — so the
/// id asked for is looked up first and, failing that, the single entity the
/// document does carry is used. Guessing between several would be a coin toss,
/// so several is `None`.
pub fn article(response: &Json, id: &str, preferred: &[&str]) -> Option<Article> {
    let entities = response.get("entities")?;
    let entity = match entities.get(id) {
        Some(entity) => entity,
        // A redirect answers under the target's id. One entity means there is
        // no ambiguity about which that is.
        None => only_value(entities)?,
    };
    let sitelinks = entity.get("sitelinks")?;
    for lang in preferred {
        // Wikidata names the site, not the language: the English Wikipedia is
        // `enwiki`, and `enwikiquote` or `enwikisource` are different projects
        // that would answer with something other than an encyclopaedia entry.
        let title = sitelinks
            .get(&format!("{lang}wiki"))
            .and_then(|s| s.field_str("title"))
            .filter(|t| !t.trim().is_empty());
        if let Some(title) = title {
            return Some(Article {
                lang: (*lang).to_string(),
                title,
            });
        }
    }
    None
}

/// Where to ask for an article's opening paragraph.
///
/// The REST summary endpoint rather than the `action=query` API: it returns the
/// lead paragraph already assembled and already plain text, where the older API
/// returns wikitext or HTML that this program would then have to strip — and
/// stripping markup to display it as fact is how a citation template ends up
/// quoted at a user.
pub fn summary_url(article: &Article) -> String {
    format!(
        "https://{}.wikipedia.org/api/rest_v1/page/summary/{}",
        article.lang,
        encode_title(&article.title)
    )
}

/// A title as it goes into a REST path.
///
/// Spaces become underscores, as every Wikipedia address does, and everything
/// outside the unreserved set is percent-encoded. Parentheses are encoded too:
/// they are legal in a path, but article titles are full of them — `Marilyn
/// Manson (band)` — and encoding them costs nothing while leaving them raw
/// depends on every intermediary agreeing they are safe.
fn encode_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for byte in title.replace(' ', "_").bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The paragraph, credited, from a REST summary document.
///
/// `None` when the page holds no extract — a disambiguation page, a redirect
/// to one, a stub that is only an infobox — which is not an error: it is the
/// same "asked, and there is nothing" the rest of the layer records.
///
/// The URL stored is the article's own canonical address when the response
/// gives one, and the address that was asked otherwise. It is never left out:
/// a `Prose` cannot be built without it, which is the point of the type.
pub fn prose(response: &Json, asked: &Article) -> Option<Prose> {
    let text = response
        .field_str("extract")
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())?;
    // The canonical page URL sits under `content_urls.desktop.page`. It is
    // preferred over the address that was asked because a title may have been
    // followed through a redirect, and a credit has to point at the article a
    // reader will actually find.
    let canonical = response
        .get("content_urls")
        .and_then(|c| c.get("desktop"))
        .and_then(|d| d.field_str("page"))
        .filter(|u| !u.is_empty());
    let lang = response
        .field_str("lang")
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| asked.lang.clone());
    Some(Prose {
        text,
        url: canonical.unwrap_or_else(|| article_url(asked)),
        lang,
        licence: LICENCE.to_string(),
    })
}

/// A readable address for an article, for when the response gives none.
fn article_url(article: &Article) -> String {
    format!(
        "https://{}.wikipedia.org/wiki/{}",
        article.lang,
        encode_title(&article.title)
    )
}

/// The only value in an object, when there is exactly one.
fn only_value(value: &Json) -> Option<&Json> {
    match value {
        Json::Obj(fields) if fields.len() == 1 => fields.values().next(),
        _ => None,
    }
}

#[cfg(test)]
#[path = "wikipedia_tests.rs"]
mod tests;
