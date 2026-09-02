//! Tests for [`super`], split out of `musicbrainz.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

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

#[test]
fn a_release_lookup_brings_the_edition_and_the_album_in_one_answer() {
    // The shape `inc=labels+release-groups` returns: the edition at the top,
    // the album folded in. Two facts on opposite sides of that line — the
    // label is the pressing's, the type and the first date are the album's —
    // and this is what makes them one request instead of two.
    let response = parse(
        r#"{
          "id": "59211ea4", "title": "Kind of Blue", "date": "1997-01-01",
          "label-info": [ { "label": { "name": "Columbia" } } ],
          "release-group": {
            "id": "c9fdb94c", "title": "Kind of Blue",
            "primary-type": "Album", "secondary-types": [],
            "first-release-date": "1959-08-17"
          }
        }"#,
    );
    let found = release(&response).expect("a release");
    assert_eq!(
        found.mbid, "c9fdb94c",
        "keyed on the album, not the pressing: two editions of one album are \
         one answer, and the edition id would file a second copy the day a CD \
         rip is replaced by a vinyl one"
    );
    assert_eq!(found.facts.label.as_deref(), Some("Columbia"));
    assert_eq!(found.facts.primary_type.as_deref(), Some("Album"));
    assert_eq!(
        found.facts.first_released.as_deref(),
        Some("1959-08-17"),
        "the album's date, not the reissue's — which is the fact a DATE tag \
         most often contradicts"
    );
}

#[test]
fn a_release_that_answers_without_its_group_is_still_kept() {
    // A lookup that succeeded is an answer. Dropping it because one requested
    // include did not come back would throw away a label for nothing.
    let response = parse(r#"{"id":"59211ea4","title":"Kind of Blue"}"#);
    let found = release(&response).expect("a release");
    assert_eq!(found.mbid, "59211ea4");
    assert_eq!(found.facts.primary_type, None);
    assert!(release(&parse(r#"{"title":"no identifier"}"#)).is_none());
}

#[test]
fn a_group_lookup_is_a_certainty_and_a_search_result_is_not() {
    let lookup = parse(
        r#"{"id":"c9fdb94c","title":"Kind of Blue","primary-type":"Album",
            "first-release-date":"1959-08-17"}"#,
    );
    let found = release_group(&lookup).expect("a group");
    assert_eq!(found.mbid, "c9fdb94c");
    assert_eq!(found.facts.first_released.as_deref(), Some("1959-08-17"));
    // The same fields, read by the same extractor, whichever request they
    // arrived in: one album read in two places is how two of them drift.
    let searched = parse(
        r#"{"release-groups":[{"id":"c9fdb94c","score":97,"title":"Kind of Blue",
            "primary-type":"Album","first-release-date":"1959-08-17"}]}"#,
    );
    assert_eq!(release_groups(&searched)[0].facts, found.facts);
}

#[test]
fn a_discography_page_carries_its_total_so_the_caller_knows_to_ask_again() {
    let response = parse(
        r#"{
          "release-group-count": 142,
          "release-group-offset": 0,
          "release-groups": [
            { "id": "c9fdb94c", "title": "Kind of Blue",
              "first-release-date": "1959-08-17",
              "primary-type": "Album", "secondary-types": [] },
            { "id": "aa11", "title": "Live at the Plugged Nickel",
              "first-release-date": "1976",
              "primary-type": "Album", "secondary-types": ["Live"] },
            { "title": "This row has no identifier" }
          ]
        }"#,
    );
    let (page, total) = discography(&response);
    assert_eq!(total, 142, "there is more than this page");
    assert_eq!(page.len(), 2, "the row with no identifier was dropped");
    assert!(page[0].is_studio_album());
    assert!(
        !page[1].is_studio_album(),
        "a live record is an Album with a secondary type, and a shelf is not \
         incomplete for lacking it"
    );
    assert_eq!(page[0].year(), Some("1959"));
    assert_eq!(
        page[1].year(),
        Some("1976"),
        "a bare year is already a year"
    );
}

#[test]
fn a_page_with_no_total_is_taken_to_be_the_whole_answer() {
    // Paging forever on a field that was never there would be one request a
    // second with no end. Stopping is the safe reading of a missing count.
    let response = parse(r#"{"release-groups":[{"id":"a","title":"One"}]}"#);
    let (page, total) = discography(&response);
    assert_eq!((page.len(), total), (1, 1));
}

#[test]
fn a_browse_asks_by_identifier_and_never_by_name() {
    let url = discography_url("561d854a", 0);
    assert!(url.contains("/release-group?artist=561d854a"), "{url}");
    assert!(!url.contains("query="), "a browse, not a search: {url}");
    assert!(url.contains("offset=0"), "{url}");
    assert!(
        discography_url("561d854a", 100).contains("offset=100"),
        "the second page asks for the second page"
    );
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
