//! Reading what MusicBrainz answers, and deciding whether it is about us.
//!
//! Deliberately **no network**. This module turns a response that somebody
//! else fetched into [`crate::sources`] records, and decides how firmly one of
//! them is attached to an entity of the catalog. Those two jobs are where the
//! mistakes live — a misread field, a wrong album picked confidently — and
//! neither of them has any reason to need a socket to be tested.
//!
//! The field spellings come from the MusicBrainz documentation and are quoted
//! exactly: `sort-name`, `life-span.begin`, `primary-type`, `secondary-types`,
//! `first-release-date`, `label-info[].label.name`. Hyphens, not underscores —
//! guessing one of those wrong produces a record that is silently empty.

use crate::json::Json;
use crate::sources::{ArtistFacts, Confidence, ReleaseFacts};
use crate::text;

/// Base address of the web service, kept here so the client has nothing to
/// decide.
pub const WEB_SERVICE: &str = "https://musicbrainz.org/ws/2";

/// The gap the service requires between two requests, with a small margin.
///
/// One request per second per address is the documented limit, and going over
/// it does not slow a run down — it makes the service answer `503` to
/// *everything* from that address until the rate drops, for every program
/// sharing it. The margin is there because the clock that decides is the
/// server's, not ours.
///
/// It lives here rather than in the client because it is a property of
/// MusicBrainz, not of how we talk to it: a build without the network feature
/// still needs it to say how long a run would take.
pub const REQUEST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1100);

/// What to ask for alongside an artist, in one request.
///
/// The whole point of a lookup over a search: these ride on the request
/// already being made, so genres, other names and the links an artist has —
/// Wikidata among them — cost nothing extra at a service that allows one
/// request per second.
///
/// `artist-rels` is deliberately absent for now. Band membership is what the
/// roadmap wants as **dated relations**, which is a change to the graph rather
/// than a field to display, and asking for data nothing can hold yet would
/// store an answer with nowhere to put it.
pub const ARTIST_INCLUDES: &str = "genres+tags+aliases+url-rels";

/// What to ask for alongside a *release*, in one request.
///
/// A release is one edition; a release group is the album every edition of it
/// belongs to. The two facts a library wants sit on opposite sides of that
/// line — the **label** is the edition's, the **type and first release date**
/// are the album's — and asking for them separately would be two requests per
/// album at one request per second.
///
/// `release-groups` folds the album into the edition's answer, so a library
/// tagged by Picard, which writes the edition identifier, gets the whole
/// record for the price of one lookup.
pub const RELEASE_INCLUDES: &str = "labels+release-groups";

/// One answer among the several a search returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate<F> {
    /// The MusicBrainz identifier, which is what makes a later fetch an update
    /// rather than a second opinion.
    pub mbid: String,
    /// Name or title as MusicBrainz spells it, for the reader to judge.
    pub name: String,
    /// Whatever the search decided about relevance, 0 to 100.
    ///
    /// **Not a probability, and not a confidence.** It says how well the query
    /// matched the index, so a library holding one album by an obscure band
    /// gets a 100 for the only thing that answered. It is one input to
    /// [`best_match`], never the verdict.
    pub score: u8,
    /// What was said, ready to be stored.
    pub facts: F,
}

/// Why nothing was attached, which is worth saying rather than returning
/// `None` and letting the caller invent a reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoMatch {
    /// The search came back with nothing at all.
    Nothing,
    /// Several answers are equally good, and choosing between them would be
    /// arbitrary. The names are carried so the report can show them.
    Ambiguous(Vec<String>),
    /// The best answer is not close enough to the name asked about.
    TooWeak {
        /// The best candidate's name, so the reader can see what was rejected.
        best: String,
        /// Its search score.
        score: u8,
    },
}

/// Below this, an answer is not worth attaching to anything.
///
/// A floor rather than a preference: MusicBrainz answers *something* for
/// almost any query, and an unfiltered "best" result is how a library ends up
/// with a Beatles record filed under a bar band of the same initials.
const FLOOR: u8 = 70;

/// Picks the answer that describes the thing asked about, or says why none does.
///
/// Two rules, both taken from behaviour this codebase already has:
///
/// - **An exact name match wins, and only widens when nothing matches
///   exactly** — the rule `Catalog::find_releases` follows. The comparison is
///   [`text::normalize`], the same function that decides two spellings are one
///   name everywhere else.
/// - **Several equally good answers are refused, not arbitrated.** Returning
///   the first of them is an arbitrary answer given without saying so, which
///   is the fault `find_releases` and `moved_to` were both fixed for.
///
/// The confidence returned is [`Confidence::Identified`] only when the caller
/// asked by identifier; a search can never produce more than
/// [`Confidence::Matched`], however sure it looks.
pub fn best_match<F: Clone>(
    candidates: &[Candidate<F>],
    wanted: &str,
) -> Result<(Candidate<F>, Confidence), NoMatch> {
    if candidates.is_empty() {
        return Err(NoMatch::Nothing);
    }
    let wanted = text::normalize(wanted);

    let exact: Vec<&Candidate<F>> = candidates
        .iter()
        .filter(|c| text::normalize(&c.name) == wanted)
        .collect();

    let pool: Vec<&Candidate<F>> = match exact.is_empty() {
        false => exact,
        true => candidates.iter().collect(),
    };

    let best = pool
        .iter()
        .max_by_key(|c| c.score)
        .expect("a non-empty pool");
    if best.score < FLOOR {
        return Err(NoMatch::TooWeak {
            best: best.name.clone(),
            score: best.score,
        });
    }

    // Two answers of the same quality are two answers. Names that normalise to
    // the same thing are not a tie — a reissue and its original often share a
    // title, and both being "right" is not the same as the program having to
    // choose.
    let tied: Vec<&&Candidate<F>> = pool
        .iter()
        .filter(|c| {
            c.score == best.score && text::normalize(&c.name) != text::normalize(&best.name)
        })
        .collect();
    if !tied.is_empty() {
        let mut names: Vec<String> = std::iter::once(best.name.clone())
            .chain(tied.iter().map(|c| c.name.clone()))
            .collect();
        names.sort();
        return Err(NoMatch::Ambiguous(names));
    }

    // The score MusicBrainz gives is about its index, not about us. A name
    // that matches exactly deserves more than one that merely ranked well, and
    // neither is ever a certainty.
    let confidence = match text::normalize(&best.name) == wanted {
        true => Confidence::matched(best.score.min(95)),
        false => Confidence::matched(best.score.saturating_sub(25)),
    };
    Ok(((*best).clone(), confidence))
}

/// Escapes the characters Lucene reads as syntax.
///
/// The search server parses the query, so a name carrying `/`, `:`, `(` or `-`
/// is not a name to it but an expression — and an expression that does not
/// parse is not a polite "nothing found": the request fails. MusicBrainz
/// documents this and gives the same example, `ac/dc` sent as `ac\/dc`, and
/// the escaping happens **before** URL-encoding, never instead of it.
///
/// This is what a whole-library run trips over on its first awkward name,
/// while a run naming one well-behaved artist works perfectly — which is
/// exactly how it was found.
pub fn escape_query(name: &str) -> String {
    const SPECIAL: [char; 22] = [
        '\\', '+', '-', '&', '|', '!', '(', ')', '{', '}', '[', ']', '^', '"', '~', '*', '?', ':',
        '/', '<', '>', '=',
    ];
    let mut out = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// --------------------------------------------------------------------------
// Reading the answers
// --------------------------------------------------------------------------

/// A string field, absent rather than empty.
fn field(value: &Json, key: &str) -> Option<String> {
    value
        .field_str(key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The search score, which MusicBrainz gives as a number 0–100.
fn score_of(value: &Json) -> u8 {
    value
        .field_u32("score")
        .unwrap_or(0)
        .min(100)
        .try_into()
        .unwrap_or(100)
}

/// Artists, as `/ws/2/artist/?query=…&fmt=json` returns them.
///
/// An entry with no identifier is skipped: without it a later fetch could not
/// tell an update from a second opinion, which is the one thing the identifier
/// is stored for.
/// The entity, read the same way whether it came from a search or a lookup.
///
/// One extractor rather than two: the two answers carry the same artist, and
/// reading it in two places is how one of them quietly stops keeping a field.
fn artist_facts(row: &Json) -> ArtistFacts {
    let life = row.get("life-span");
    ArtistFacts {
        // `country` is a code, `area.name` is the name a reader wants. Prefer
        // the name, fall back to the code rather than losing the fact.
        area: row
            .get("area")
            .and_then(|a| field(a, "name"))
            .or_else(|| field(row, "country")),
        began: life.and_then(|l| field(l, "begin")),
        // An artist that has not ended has no end date, and a `life-span` may
        // carry `ended: false` with no `end`.
        ended: life.and_then(|l| field(l, "end")),
        // `ended: false` is an answer — "still going" — and it is the one a
        // reader looking at a band wants most.
        active: life
            .and_then(|l| l.field_optional_bool("ended"))
            .map(|e| !e),
        kind: field(row, "type"),
        disambiguation: field(row, "disambiguation"),
        // `genres` and `tags` have the same shape; genres are the curated
        // half, so they come first and tags fill in only when there are none.
        genres: voted(row, "genres")
            .or_else(|| voted(row, "tags"))
            .unwrap_or_default(),
        aliases: row
            .get("aliases")
            .and_then(Json::as_arr)
            .map(|a| a.iter().filter_map(|x| field(x, "name")).collect())
            .unwrap_or_default(),
        wikidata: linked(row, "wikidata"),
        discogs: linked(row, "discogs"),
        // MusicBrainz spells this relationship "official homepage".
        homepage: linked(row, "official homepage"),
        // Filled by its own pass, never by this one: a discography is a
        // browse over release groups, not a field of an artist lookup, and
        // reading it here would quietly empty it on every ordinary fetch.
        discography: Vec::new(),
        // MusicBrainz holds no prose about an artist: an annotation there is
        // an editorial note about the data, not a description of the
        // musician. The summary comes from Wikipedia, reached through the
        // `wikidata` link above, and carries its own licence with it.
        summary: None,
    }
}

/// Names from a `genres` or `tags` list, most agreed first.
///
/// `None` when the list is absent — which is not the same as an artist nobody
/// tagged, and only the first of those means "we did not ask for it".
fn voted(row: &Json, key: &str) -> Option<Vec<String>> {
    let rows = row.get(key)?.as_arr()?;
    let mut scored: Vec<(u32, String)> = rows
        .iter()
        .filter_map(|t| Some((t.field_u32("count").unwrap_or(0), field(t, "name")?)))
        .collect();
    // Most agreed first, then alphabetically so two runs give the same order.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Some(scored.into_iter().map(|(_, name)| name).collect())
}

/// The URL behind a relationship of the given type.
///
/// `relations[].url.resource` is the shape a URL relationship takes, and the
/// one spelling here I could not confirm against a live answer — so it is
/// written to come back empty rather than to break, and the tests below pin
/// what it expects.
fn linked(row: &Json, relation: &str) -> Option<String> {
    row.get("relations")?.as_arr()?.iter().find_map(|r| {
        (r.field_str("type").as_deref() == Some(relation))
            .then(|| r.get("url").and_then(|u| field(u, "resource")))
            .flatten()
    })
}

/// One artist, as `/ws/2/artist/{mbid}?fmt=json` returns it.
///
/// A **lookup**, not a search, and the difference is the whole reason this
/// exists beside [`artists`]. A search answers with an abbreviated entry and a
/// relevance score, because it has to guess which artist was meant. A lookup
/// is asked about one identifier and answers with the entity itself —
/// `life-span.ended` and `disambiguation` among the fields a search result
/// leaves out.
///
/// Which matters because a library tagged with Picard already carries those
/// identifiers. Searching by name for an artist whose MBID sits in the tags is
/// asking a question that has already been answered, and accepting a guess in
/// place of a certainty.
pub fn artist(response: &Json) -> Option<Candidate<ArtistFacts>> {
    let mbid = field(response, "id")?;
    Some(Candidate {
        mbid,
        name: field(response, "name").unwrap_or_default(),
        // Nothing was ranked: the service was asked about this one thing.
        score: 100,
        facts: artist_facts(response),
    })
}

/// Artists, as `/ws/2/artist/?query=…&fmt=json` returns them.
///
/// A **search**: the answers are ranked guesses at what was meant, and each
/// entry is abbreviated. Prefer [`artist`] whenever an identifier is known.
///
/// An entry with no identifier is skipped: without it a later fetch could not
/// tell an update from a second opinion, which is the one thing the identifier
/// is stored for.
pub fn artists(response: &Json) -> Vec<Candidate<ArtistFacts>> {
    let rows = response
        .get("artists")
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    rows.iter()
        .filter_map(|row| {
            let mbid = field(row, "id")?;
            Some(Candidate {
                mbid,
                name: field(row, "name").unwrap_or_default(),
                score: score_of(row),
                facts: artist_facts(row),
            })
        })
        .collect()
}

/// Release groups, as `/ws/2/release-group/?query=…&fmt=json` returns them.
///
/// Groups rather than releases, deliberately: a release group is *the album*,
/// and every remaster and reissue of it is a release inside that group. Asking
/// about releases would answer with one edition among twenty and call it the
/// album's date.
pub fn release_groups(response: &Json) -> Vec<Candidate<ReleaseFacts>> {
    let rows = response
        .get("release-groups")
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    rows.iter()
        .filter_map(|row| {
            let mbid = field(row, "id")?;
            Some(Candidate {
                mbid,
                name: field(row, "title").unwrap_or_default(),
                score: score_of(row),
                facts: release_facts(row),
            })
        })
        .collect()
}

/// The album, read the same way wherever the group came from.
///
/// One extractor for the search result, the group lookup and the group folded
/// into a release lookup — for the reason [`artist_facts`] is one function:
/// reading the same entity in three places is how two of them quietly stop
/// keeping a field.
///
/// The label is deliberately **not** read here. A group has none; a release
/// does. Filling it from whichever edition answered first would attribute one
/// pressing's label to the album itself.
fn release_facts(group: &Json) -> ReleaseFacts {
    ReleaseFacts {
        primary_type: field(group, "primary-type"),
        secondary_types: group
            .get("secondary-types")
            .and_then(Json::as_arr)
            .map(|a| a.iter().filter_map(Json::as_string).collect())
            .unwrap_or_default(),
        first_released: field(group, "first-release-date"),
        label: None,
    }
}

/// One release group, as a lookup answers: `/ws/2/release-group/<mbid>`.
///
/// The counterpart of [`artist`]: an answer about the identifier that was
/// asked for is a certainty, where a search result is a guess with a score.
/// Used when the tags carry `MUSICBRAINZ_RELEASEGROUPID` but no edition.
pub fn release_group(response: &Json) -> Option<Candidate<ReleaseFacts>> {
    let mbid = field(response, "id")?;
    Some(Candidate {
        mbid,
        name: field(response, "title").unwrap_or_default(),
        score: 100,
        facts: release_facts(response),
    })
}

/// One release, as a lookup with [`RELEASE_INCLUDES`] answers.
///
/// The richest answer this program can get about an album, and the cheapest:
/// the edition supplies the label, the folded release group supplies the type
/// and the date the album first appeared — which is the fact a reissue's `DATE`
/// tag most often contradicts.
///
/// The identifier kept is the **release group's**, not the edition's. Two
/// pressings of one album are one album, and keying the record on the edition
/// would file a second copy of the same answer the day the user replaces their
/// CD rip with a vinyl one. The edition is what was asked; the album is what
/// was learnt.
pub fn release(response: &Json) -> Option<Candidate<ReleaseFacts>> {
    let group = response.get("release-group");
    let mut facts = group.map(release_facts).unwrap_or_default();
    facts.label = label_of_release(response);
    // Falling back to the edition's own identifier: an answer with no group is
    // not one this program has seen, but storing it under the edition is
    // better than dropping a lookup that succeeded.
    let mbid = group
        .and_then(|g| field(g, "id"))
        .or_else(|| field(response, "id"))?;
    Some(Candidate {
        mbid,
        name: group
            .and_then(|g| field(g, "title"))
            .or_else(|| field(response, "title"))
            .unwrap_or_default(),
        score: 100,
        facts,
    })
}

/// How many release groups one browse request may return.
///
/// The service's own ceiling. A prolific artist has more than this, so the
/// caller pages — which is the one place in this program where a single entity
/// can cost more than one request, and the reason [`discography`] hands back
/// the total alongside the page.
pub const BROWSE_LIMIT: usize = 100;

/// Where to ask for everything credited to an artist.
///
/// A **browse**, not a search: browse answers "what does this identifier have",
/// which is a question with one right answer, where a search answers "what
/// resembles these words". Nothing here is scored or matched, because nothing
/// here is guessed.
///
/// `type=album` narrows what travels. It is a saving and not the filter that
/// matters — [`crate::sources::KnownRelease::is_studio_album`] decides what a
/// reader is shown, so a parameter the service ignores costs bandwidth and
/// never correctness.
pub fn discography_url(artist_mbid: &str, offset: usize) -> String {
    format!(
        "{WEB_SERVICE}/release-group?artist={artist_mbid}&type=album\
         &fmt=json&limit={BROWSE_LIMIT}&offset={offset}"
    )
}

/// One page of an artist's discography, and how many there are in all.
///
/// The total comes back as `release-group-count` and is what tells the caller
/// whether to ask for another page. Absent, it is taken to be what arrived:
/// stopping is the safe reading, since paging forever on a field that was never
/// there would be a request per second with no end.
pub fn discography(response: &Json) -> (Vec<crate::sources::KnownRelease>, usize) {
    use crate::sources::KnownRelease;
    let rows = response
        .get("release-groups")
        .and_then(Json::as_arr)
        .unwrap_or(&[]);
    let page: Vec<KnownRelease> = rows
        .iter()
        .filter_map(|row| {
            // No identifier, no row: the only way to tell "you own this one"
            // from "you are missing it" without an identifier is the title, and
            // two records share a title often enough that a wish list built on
            // titles alone is wrong.
            Some(KnownRelease {
                mbid: field(row, "id")?,
                title: field(row, "title").unwrap_or_default(),
                first_released: field(row, "first-release-date"),
                primary_type: field(row, "primary-type"),
                secondary_types: row
                    .get("secondary-types")
                    .and_then(Json::as_arr)
                    .map(|a| a.iter().filter_map(Json::as_string).collect())
                    .unwrap_or_default(),
            })
        })
        .collect();
    let total = response
        .field_u32("release-group-count")
        .map(|n| n as usize)
        .unwrap_or(page.len());
    (page, total)
}

/// The label of a release, as a release lookup returns it.
///
/// Separate from [`release_groups`] because it comes from a different request,
/// and because it describes an edition rather than the album.
pub fn label_of_release(response: &Json) -> Option<String> {
    response
        .get("label-info")
        .and_then(Json::as_arr)?
        .iter()
        .find_map(|info| info.get("label").and_then(|l| field(l, "name")))
}

#[cfg(test)]
#[path = "musicbrainz_tests.rs"]
mod tests;
