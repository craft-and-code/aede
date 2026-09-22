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
use crate::model::CreditAttribute;
use crate::sources::{
    ArtistFacts, Confidence, CreditLink, LabelFacts, Membership, ReleaseFacts, Side, TrackFacts,
    WorkLink,
};
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
/// `artist-rels` carries the dated memberships — who played in a band and
/// between which years — read by `memberships` below. It rides on the same request
/// as the rest, so a line-up costs nothing beyond the lookup already being
/// made, and the dates are what let an album page name the band as it stood
/// the year that record came out.
pub const ARTIST_INCLUDES: &str = "genres+tags+aliases+url-rels+artist-rels";

/// What a recording lookup needs for performance, production and composition
/// credits in a single request.
///
/// `work-level-rels` is a switch: `work-rels` first attaches each work and
/// `artist-rels` then asks for artist relationships both on the recording and
/// on those linked works.
pub const RECORDING_INCLUDES: &str = "artist-rels+work-rels+work-level-rels";

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
        // `country` is a code, `area.name` is the name a reader wants. Both
        // are kept: the name is what a listing shows, the code is what
        // somebody types. Preferring one and discarding the other threw away
        // a fact the source had stated — and it was the short one, which is
        // the half a reader reaches for.
        area: row
            .get("area")
            .and_then(|a| field(a, "name"))
            .or_else(|| field(row, "country")),
        country_code: field(row, "country"),
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
        members: memberships(row),
        // MusicBrainz holds no prose about an artist: an annotation there is
        // an editorial note about the data, not a description of the
        // musician. The summary comes from Wikipedia, reached through the
        // `wikidata` link above, and carries its own licence with it.
        summary: None,
        // Likewise filled by its own pass: MusicBrainz holds no picture of an
        // artist either, and `fetch --portraits` reaches one the same way
        // Wikipedia is reached, through the `wikidata` link above.
        portrait: None,
        // Same story again: `fetch --logos` is the only pass that fills this.
        logo: None,
    }
}

/// Band memberships, from the `artist-rels` a lookup was asked for.
///
/// # The direction, measured rather than assumed
///
/// MusicBrainz states **one** relation between two artists and returns it on
/// both, distinguished by `direction`. Reading that field wrongly does not lose
/// data, it inverts it — Black Sabbath would appear in the list of Ozzy
/// Osbourne's members — so it was checked against live answers for a person and
/// for a band rather than reasoned about.
///
/// Both answers carry `"direction": "backward"`, and that settles it: these
/// relationships are defined **from the musician towards the group**, so a
/// backward one is being read *from the group's end* and the artist it names is
/// the player. The first version of this function had it the other way round,
/// on the reasonable-sounding assumption that a person's own record would read
/// forward. It does not.
///
/// # Which relationships count
///
/// `member of band` is the obvious one and it is not enough. Ozzy Osbourne's
/// own record holds no members at all — a solo artist is not a band — and every
/// musician who played on his records is there as an
/// `instrumental supporting musician`: Randy Rhoads on guitar, Bob Daisley on
/// bass. A line-up that showed nothing for him while MusicBrainz plainly holds
/// his band would be a worse answer than no feature.
///
/// The two are kept apart rather than merged, because a founding member and a
/// guitarist hired for one tour are both on the record and only one of them was
/// in the band. [`Membership::kind`] carries the source's own phrase.
///
/// **The list is what has been seen in a real answer**, and nothing else: a
/// relationship type nobody has checked is a guess with a `const` around it.
fn memberships(row: &Json) -> Vec<Membership> {
    let Some(relations) = row.get("relations").and_then(Json::as_arr) else {
        return Vec::new();
    };
    relations
        .iter()
        .filter(|r| {
            r.field_str("type")
                .is_some_and(|t| MEMBER_RELATIONS.contains(&t.as_str()))
        })
        .filter_map(|r| {
            let artist = r.get("artist")?;
            Some(Membership {
                // No identifier, no membership: a name alone cannot be
                // followed to the artist it names, and the whole value of this
                // list is that each row leads somewhere.
                mbid: field(artist, "id")?,
                name: field(artist, "name").unwrap_or_default(),
                kind: field(r, "type").unwrap_or_default(),
                side: match r.field_str("direction").as_deref() {
                    Some("backward") => Side::Player,
                    _ => Side::Group,
                },
                attributes: r
                    .get("attributes")
                    .and_then(Json::as_arr)
                    .map(|a| a.iter().filter_map(Json::as_string).collect())
                    .unwrap_or_default(),
                // `begin` may be a year, `1979-11`, or a full date, and every
                // one of those is answered by taking the first four characters
                // where a year is wanted.
                began: field(r, "begin"),
                ended: field(r, "end"),
                // `ended: false` is an answer — "still in the band" — and it is
                // the one a reader looking at a line-up wants most. It arrives
                // beside `"end": null`, so the two must be read separately.
                over: r.field_optional_bool("ended"),
            })
        })
        .collect()
}

/// The relationships this reads, spelt as MusicBrainz spells them.
///
/// One place, because the strings are the whole filter: a typo here empties
/// every line-up in the program and nothing else goes wrong. Each was seen in
/// a live answer — `member of band` on Judas Priest, `instrumental supporting
/// musician` on Ozzy Osbourne — and a type nobody has checked does not belong
/// on this list, however plausible its name.
const MEMBER_RELATIONS: [&str; 2] = ["member of band", "instrumental supporting musician"];

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

/// One recording, as a lookup with [`RECORDING_INCLUDES`] returns it.
///
/// A lookup is evidence about the identifier we asked for, never a title
/// match. Its recording credits, work relationships and work credits remain
/// source facts: reconciliation decides later whether and how they join the
/// local canonical graph.
pub fn recording(response: &Json) -> Option<Candidate<TrackFacts>> {
    let mbid = field(response, "id")?;
    let credits = relationship_credits(response);
    let works = response
        .get("relations")
        .and_then(Json::as_arr)
        .map(|relations| {
            relations
                .iter()
                .filter(|relation| relation.field_str("target-type").as_deref() == Some("work"))
                .filter_map(|relation| {
                    let work = relation.get("work")?;
                    Some(WorkLink {
                        mbid: field(work, "id")?,
                        title: field(work, "title").unwrap_or_default(),
                        relation_id: field(relation, "id"),
                        relation_type: field(relation, "type"),
                        relation_type_id: field(relation, "type-id"),
                        direction: field(relation, "direction"),
                        attributes: relationship_attributes(relation),
                        credits: relationship_credits(work),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Candidate {
        mbid: mbid.clone(),
        name: field(response, "title").unwrap_or_default(),
        score: 100,
        facts: TrackFacts {
            recording: Some(mbid),
            title: field(response, "title"),
            works,
            credits,
            relationships_complete: true,
            ..Default::default()
        },
    })
}

/// Artist relationships carried by one recording or work response.
fn relationship_credits(entity: &Json) -> Vec<CreditLink> {
    entity
        .get("relations")
        .and_then(Json::as_arr)
        .map(|relations| {
            relations
                .iter()
                .filter(|relation| relation.field_str("target-type").as_deref() == Some("artist"))
                .filter_map(|relation| {
                    let artist = relation.get("artist")?;
                    let credited_as = field(relation, "target-credit").filter(|s| !s.is_empty());
                    Some(CreditLink {
                        relation_id: field(relation, "id"),
                        role_id: field(relation, "type-id"),
                        role: field(relation, "type")?,
                        direction: field(relation, "direction"),
                        artist_mbid: field(artist, "id")?,
                        artist_name: field(artist, "name").unwrap_or_default(),
                        credited_as,
                        attributes: relationship_attributes(relation),
                        began: field(relation, "begin"),
                        ended: field(relation, "end"),
                        over: relation.field_optional_bool("ended"),
                        order: relation.field_u32("ordering-key"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Attributes of one relationship, retaining values and credited spellings.
fn relationship_attributes(relation: &Json) -> Vec<CreditAttribute> {
    relation
        .get("attributes")
        .and_then(Json::as_arr)
        .map(|attributes| {
            attributes
                .iter()
                .filter_map(Json::as_string)
                .map(|name| CreditAttribute {
                    id: relation
                        .get("attribute-ids")
                        .and_then(|ids| ids.get(&name))
                        .and_then(Json::as_string),
                    value: relation
                        .get("attribute-values")
                        .and_then(|values| values.get(&name))
                        .and_then(Json::as_string),
                    credited_as: relation
                        .get("attribute-credits")
                        .and_then(|credits| credits.get(&name))
                        .and_then(Json::as_string)
                        .filter(|value| !value.is_empty()),
                    name,
                })
                .collect()
        })
        .unwrap_or_default()
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
        label_mbid: None,
        // MusicBrainz holds no artwork: the images live at the Cover Art
        // Archive, which is a different service with its own answer — see
        // [`crate::coverart`].
        cover_art: None,
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
    if let Some(label) = label_of_release(response) {
        facts.label = Some(label.name);
        facts.label_mbid = label.mbid;
    }
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

/// A label credited on a release: its name and, when the same entry carried
/// one, its MusicBrainz identifier.
pub struct LabelOfRelease {
    /// The name, as the release lookup spells it.
    pub name: String,
    /// The label's own MusicBrainz identifier, when the same entry named
    /// one.
    pub mbid: Option<String>,
}

/// The label of a release, as a release lookup returns it.
///
/// Separate from [`release_groups`] because it comes from a different request,
/// and because it describes an edition rather than the album.
///
/// The name and the identifier are read from **the same** `label-info` entry
/// — the first one that names a label — never a name from one row and an
/// identifier from another: a release crediting several labels would
/// otherwise risk pairing the name of one with the address of a different
/// one. No tag carries a label's MusicBrainz identifier the way
/// `MUSICBRAINZ_ARTISTID` does an artist's, so this is the only place one is
/// ever read from.
pub fn label_of_release(response: &Json) -> Option<LabelOfRelease> {
    response
        .get("label-info")
        .and_then(Json::as_arr)?
        .iter()
        .find_map(|info| {
            let label = info.get("label")?;
            let name = field(label, "name")?;
            Some(LabelOfRelease {
                name,
                mbid: field(label, "id"),
            })
        })
}

/// One label, as `/ws/2/label/{mbid}?fmt=json` returns it.
///
/// A **lookup**, asked only once an identifier is already known — see
/// `crate::sources::ReleaseFacts::label_mbid` and the `fetch --labels`
/// module doc for where that comes from: a release this catalog looked up
/// sometimes already names its label's own identifier, and a lookup then
/// turns that into a certainty instead of guessing by name all over again.
/// No tag ever carries it directly, so unlike [`artist`] this is never asked
/// with a value read out of the files.
pub fn label(response: &Json) -> Option<Candidate<LabelFacts>> {
    let mbid = field(response, "id")?;
    Some(Candidate {
        mbid,
        name: field(response, "name").unwrap_or_default(),
        // Nothing was ranked: the service was asked about this one thing.
        score: 100,
        facts: LabelFacts::default(),
    })
}

/// Labels, as `/ws/2/label/?query=…&fmt=json` returns them.
///
/// A **search**: ranked guesses, asked when no identifier is known for this
/// label yet. Prefer [`label`] whenever one is.
pub fn labels(response: &Json) -> Vec<Candidate<LabelFacts>> {
    let rows = response.get("labels").and_then(Json::as_arr).unwrap_or(&[]);
    rows.iter()
        .filter_map(|row| {
            let mbid = field(row, "id")?;
            Some(Candidate {
                mbid,
                name: field(row, "name").unwrap_or_default(),
                score: score_of(row),
                facts: LabelFacts::default(),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "musicbrainz_tests.rs"]
mod tests;
