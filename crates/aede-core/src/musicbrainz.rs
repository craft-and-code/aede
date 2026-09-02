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
                facts: ReleaseFacts {
                    primary_type: field(row, "primary-type"),
                    secondary_types: row
                        .get("secondary-types")
                        .and_then(Json::as_arr)
                        .map(|a| a.iter().filter_map(Json::as_string).collect())
                        .unwrap_or_default(),
                    first_released: field(row, "first-release-date"),
                    // A group has no label — a release does. Left empty here
                    // rather than filled from whichever edition answered
                    // first, which would attribute one pressing's label to the
                    // album itself.
                    label: None,
                },
            })
        })
        .collect()
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
mod tests {
    use super::*;

    fn parse(text: &str) -> Json {
        crate::json::parse(text).expect("valid JSON")
    }

    #[test]
    fn an_artist_search_is_read_field_by_field() {
        // The spellings are MusicBrainz's, hyphens included. Getting one wrong
        // does not fail: it produces a record that is quietly empty, which is
        // exactly what a test has to catch.
        let response = parse(
            r#"{
              "created": "2026-09-01T12:00:00.000Z", "count": 2, "offset": 0,
              "artists": [
                { "id": "561d854a-6a28-4aa7-8c99-323e6ce46c2a", "score": 100,
                  "name": "Miles Davis", "sort-name": "Davis, Miles",
                  "type": "Person", "country": "US",
                  "area": { "id": "489", "name": "United States" },
                  "life-span": { "begin": "1926-05-26", "end": "1991-09-28",
                                 "ended": true } },
                { "id": "aaaaaaa", "score": 61, "name": "Miles Davis Quintet",
                  "type": "Group",
                  "life-span": { "begin": "1955", "ended": false } }
              ]
            }"#,
        );
        let found = artists(&response);
        assert_eq!(found.len(), 2);

        assert_eq!(found[0].mbid, "561d854a-6a28-4aa7-8c99-323e6ce46c2a");
        assert_eq!(found[0].score, 100);
        assert_eq!(found[0].facts.kind.as_deref(), Some("Person"));
        assert_eq!(found[0].facts.area.as_deref(), Some("United States"));
        assert_eq!(found[0].facts.began.as_deref(), Some("1926-05-26"));
        assert_eq!(found[0].facts.ended.as_deref(), Some("1991-09-28"));

        // A group still going has a beginning and no end; `ended: false` must
        // not turn into an end date.
        assert_eq!(found[1].facts.began.as_deref(), Some("1955"));
        assert_eq!(found[1].facts.ended, None);
    }

    #[test]
    fn a_missing_area_falls_back_to_the_country_code() {
        let response =
            parse(r#"{"artists":[{"id":"x","score":90,"name":"Kraftwerk","country":"DE"}]}"#);
        let found = artists(&response);
        assert_eq!(found[0].facts.area.as_deref(), Some("DE"));
    }

    #[test]
    fn an_entry_without_an_identifier_is_skipped() {
        // The identifier is what makes a second fetch an update. A record
        // without one would duplicate itself on every run.
        let response = parse(r#"{"artists":[{"score":100,"name":"Nobody"}]}"#);
        assert!(artists(&response).is_empty());
    }

    #[test]
    fn a_release_group_carries_its_types_and_not_a_label() {
        let response = parse(
            r#"{
              "release-groups": [
                { "id": "c9fdb94c", "score": 100, "title": "The Lost Tape",
                  "first-release-date": "2012-05-22", "primary-type": "Album",
                  "secondary-types": ["Mixtape/Street", "Live"] }
              ]
            }"#,
        );
        let found = release_groups(&response);
        assert_eq!(found[0].facts.primary_type.as_deref(), Some("Album"));
        assert_eq!(
            found[0].facts.secondary_types,
            vec!["Mixtape/Street", "Live"]
        );
        assert_eq!(found[0].facts.first_released.as_deref(), Some("2012-05-22"));
        // A group has no label: filling it from an edition would attribute one
        // pressing's label to the album itself.
        assert_eq!(found[0].facts.label, None);
    }

    #[test]
    fn a_label_comes_from_a_release_and_not_from_the_group() {
        let response = parse(
            r#"{"id":"59211ea4","title":"x","date":"2003-12-04",
                "label-info":[{"label":{"name":"Phonometrography"}}]}"#,
        );
        assert_eq!(
            label_of_release(&response).as_deref(),
            Some("Phonometrography")
        );
        assert_eq!(label_of_release(&parse(r#"{"id":"x"}"#)), None);
    }

    fn candidate(name: &str, score: u8) -> Candidate<ArtistFacts> {
        Candidate {
            mbid: format!("id-{name}"),
            name: name.to_string(),
            score,
            facts: ArtistFacts::default(),
        }
    }

    #[test]
    fn a_name_is_a_name_and_not_an_expression() {
        // Found the hard way: a whole-library run failed on its first request
        // while naming one artist worked, because the search server parses the
        // query and an unescaped `/` or `:` makes it unparseable. The failure
        // arrives as 503, which reads as "you are going too fast".
        assert_eq!(escape_query("AC/DC"), r"AC\/DC");
        assert_eq!(
            escape_query("Godspeed You! Black Emperor"),
            r"Godspeed You\! Black Emperor"
        );
        assert_eq!(escape_query("Sunn O)))"), r"Sunn O\)\)\)");
        assert_eq!(
            escape_query("Emerson, Lake & Palmer"),
            r"Emerson, Lake \& Palmer"
        );
        assert_eq!(escape_query("X-Ray Spex"), r"X\-Ray Spex");
        assert_eq!(
            escape_query("Miles Davis"),
            "Miles Davis",
            "an ordinary name is untouched"
        );
    }

    #[test]
    fn a_lookup_carries_the_genres_the_aliases_and_the_links() {
        // All of it from the same request: `inc=genres+tags+aliases+url-rels`
        // costs no extra call, which is what makes it worth doing at all when
        // the service allows one request per second.
        let response = parse(
            r#"{
              "id": "c98ff0e1", "name": "Marilyn Manson", "type": "Group",
              "disambiguation": "US industrial metal band",
              "area": { "name": "United States" },
              "life-span": { "begin": "1989", "ended": false },
              "genres": [ { "name": "industrial metal", "count": 12 },
                          { "name": "shock rock", "count": 4 } ],
              "tags": [ { "name": "ignored when genres exist", "count": 99 } ],
              "aliases": [ { "name": "Marilyn Manson and the Spooky Kids" } ],
              "relations": [
                { "type": "wikidata",
                  "url": { "resource": "https://www.wikidata.org/wiki/Q152388" } },
                { "type": "official homepage",
                  "url": { "resource": "https://www.marilynmanson.com/" } },
                { "type": "discogs",
                  "url": { "resource": "https://www.discogs.com/artist/12345" } }
              ]
            }"#,
        );
        let found = artist(&response).expect("an artist");
        assert_eq!(found.facts.genres, vec!["industrial metal", "shock rock"]);
        assert_eq!(
            found.facts.aliases,
            vec!["Marilyn Manson and the Spooky Kids"]
        );
        assert_eq!(
            found.facts.wikidata.as_deref(),
            Some("https://www.wikidata.org/wiki/Q152388"),
            "the link the next milestone is built on"
        );
        assert_eq!(
            found.facts.homepage.as_deref(),
            Some("https://www.marilynmanson.com/")
        );
        assert!(found.facts.discogs.is_some());
    }

    #[test]
    fn tags_stand_in_only_where_no_genre_was_voted() {
        // Genres are the curated half of the same list. Merging the two would
        // put "seen live" beside "industrial metal" as though a crowd had
        // meant the same kind of thing by both.
        let tagged = parse(
            r#"{"id":"x","name":"Someone",
                "tags":[{"name":"seen live","count":3},{"name":"folk","count":7}]}"#,
        );
        assert_eq!(
            artist(&tagged).unwrap().facts.genres,
            vec!["folk", "seen live"]
        );

        // And an absent list is not an artist nobody tagged: it is a question
        // that was not asked.
        let bare = parse(r#"{"id":"y","name":"Someone"}"#);
        assert!(artist(&bare).unwrap().facts.genres.is_empty());
    }

    #[test]
    fn a_relationship_of_another_kind_is_not_mistaken_for_a_link() {
        let response = parse(
            r#"{"id":"x","name":"Someone","relations":[
                {"type":"free streaming","url":{"resource":"https://example.org/s"}}]}"#,
        );
        let found = artist(&response).expect("an artist");
        assert_eq!(found.facts.wikidata, None);
        assert_eq!(found.facts.discogs, None);
        assert_eq!(found.facts.homepage, None);
    }

    #[test]
    fn a_band_that_never_split_says_so() {
        // `ended: false` is the answer a reader looking at a band wants most,
        // and dropping it leaves "formed 1989" saying neither that they split
        // nor that they did not.
        let still = parse(
            r#"{"artists":[{"id":"x","score":100,"name":"Marilyn Manson",
                "life-span":{"begin":"1989","ended":false}}]}"#,
        );
        let found = artists(&still);
        assert_eq!(found[0].facts.active, Some(true));
        assert_eq!(found[0].facts.ended, None);

        let gone = parse(
            r#"{"artists":[{"id":"y","score":100,"name":"Miles Davis",
                "life-span":{"begin":"1926","end":"1991-09-28","ended":true}}]}"#,
        );
        assert_eq!(artists(&gone)[0].facts.active, Some(false));

        // No `life-span` at all is a silence, not an answer.
        let silent = parse(r#"{"artists":[{"id":"z","score":100,"name":"Someone"}]}"#);
        assert_eq!(artists(&silent)[0].facts.active, None);
    }

    #[test]
    fn an_exact_name_wins_over_a_higher_ranked_one() {
        // The rule `find_releases` already follows: an exact match ends the
        // search, and only its absence widens it. MusicBrainz ranking a
        // longer name higher must not rename the artist being asked about.
        let found = [
            candidate("Miles Davis Quintet", 100),
            candidate("Miles Davis", 88),
        ];
        let (best, confidence) = best_match(&found, "Miles Davis").expect("a match");
        assert_eq!(best.name, "Miles Davis");
        assert_eq!(confidence, Confidence::matched(88));
    }

    #[test]
    fn a_search_never_produces_a_certainty() {
        // The roadmap's rule: a value reached by matching is never treated as
        // identified, however well it scored.
        let found = [candidate("Miles Davis", 100)];
        let (_, confidence) = best_match(&found, "Miles Davis").expect("a match");
        assert!(!confidence.is_certain(), "confidence: {confidence:?}");
        assert_eq!(confidence, Confidence::matched(95), "and it is capped");
    }

    #[test]
    fn a_name_that_only_ranked_well_is_trusted_less() {
        let found = [candidate("Miles Davis Quintet", 90)];
        let (best, confidence) = best_match(&found, "Miles Davis").expect("a match");
        assert_eq!(best.name, "Miles Davis Quintet");
        assert_eq!(
            confidence,
            Confidence::matched(65),
            "ranked well, but it is not the name that was asked about"
        );
    }

    #[test]
    fn two_equally_good_answers_are_refused_rather_than_arbitrated() {
        // Returning the first is an arbitrary answer given without saying so —
        // the fault `find_releases` and the moved-file rescue were both fixed
        // for. Here it would file one band's country onto another.
        let found = [candidate("Nirvana", 90), candidate("Nirvana (UK)", 90)];
        let refused = best_match(&found, "grunge band").expect_err("no arbitration");
        match refused {
            NoMatch::Ambiguous(names) => {
                assert_eq!(names, vec!["Nirvana", "Nirvana (UK)"], "both are named");
            }
            other => panic!("expected an ambiguity, got {other:?}"),
        }
    }

    #[test]
    fn a_weak_best_answer_is_no_answer() {
        // MusicBrainz answers something for almost any query. Without a floor,
        // a misspelt folder name attaches a stranger's discography.
        let found = [candidate("Someone Else Entirely", 42)];
        assert_eq!(
            best_match(&found, "Miles Davis"),
            Err(NoMatch::TooWeak {
                best: "Someone Else Entirely".to_string(),
                score: 42
            })
        );
        assert_eq!(
            best_match::<ArtistFacts>(&[], "Miles Davis"),
            Err(NoMatch::Nothing)
        );
    }

    #[test]
    fn two_spellings_of_one_name_are_not_an_ambiguity() {
        // A reissue and its original share a title, and two answers that
        // normalise to the same name are not the program having to choose.
        let found = [
            candidate("The Beatles", 100),
            candidate("Beatles, The", 100),
        ];
        let (best, _) = best_match(&found, "The Beatles").expect("a match");
        assert!(best.name.contains("Beatles"));
    }
}
