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
fn a_recording_lookup_keeps_only_explicit_work_relationships() {
    let response = parse(
        r#"{
          "id":"recording-id", "title":"All Along the Watchtower",
          "relations":[
            {"id":"performance-rel", "type-id":"performance-type",
             "type":"performance", "target-type":"work", "direction":"forward",
             "attributes":["cover"], "attribute-ids":{"cover":"cover-type"},
             "work":{"id":"work-id", "title":"All Along the Watchtower",
               "relations":[
                 {"id":"composer-rel", "type-id":"composer-type", "type":"composer",
                  "target-type":"artist", "direction":"backward", "ordering-key":1,
                  "target-credit":"Robert Dylan", "begin":"1967", "end":null,
                  "ended":false, "attributes":[],
                  "artist":{"id":"dylan-id", "name":"Bob Dylan"}}
               ]}},
            {"id":"guitar-rel", "type-id":"instrument-type", "type":"instrument",
             "target-type":"artist", "direction":"backward",
             "target-credit":"Jimi Hendrix",
             "attributes":["guitar"],
             "attribute-ids":{"guitar":"guitar-type"},
             "attribute-values":{"guitar":"electric guitar"},
             "attribute-credits":{"guitar":"Fender Stratocaster"},
             "artist":{"id":"hendrix-id", "name":"Jimi Hendrix"}}
          ]
        }"#,
    );
    let found = recording(&response).expect("recording");
    assert_eq!(found.mbid, "recording-id");
    assert_eq!(found.facts.recording.as_deref(), Some("recording-id"));
    assert_eq!(found.facts.works.len(), 1);
    assert_eq!(found.facts.works[0].mbid, "work-id");
    assert_eq!(found.facts.works[0].title, "All Along the Watchtower");
    assert_eq!(
        found.facts.works[0].relation_id.as_deref(),
        Some("performance-rel")
    );
    assert_eq!(
        found.facts.works[0].relation_type_id.as_deref(),
        Some("performance-type")
    );
    assert_eq!(found.facts.works[0].direction.as_deref(), Some("forward"));
    assert_eq!(found.facts.works[0].attributes[0].name, "cover");
    assert_eq!(
        found.facts.works[0].attributes[0].id.as_deref(),
        Some("cover-type")
    );
    assert_eq!(found.facts.works[0].credits.len(), 1);
    let composer = &found.facts.works[0].credits[0];
    assert_eq!(composer.role, "composer");
    assert_eq!(composer.artist_mbid, "dylan-id");
    assert_eq!(composer.credited_as.as_deref(), Some("Robert Dylan"));
    assert_eq!(composer.began.as_deref(), Some("1967"));
    assert_eq!(composer.order, Some(1));

    assert_eq!(found.facts.credits.len(), 1);
    let performer = &found.facts.credits[0];
    assert_eq!(performer.role, "instrument");
    assert_eq!(performer.direction.as_deref(), Some("backward"));
    assert_eq!(performer.relation_id.as_deref(), Some("guitar-rel"));
    assert_eq!(performer.attributes[0].name, "guitar");
    assert_eq!(performer.attributes[0].id.as_deref(), Some("guitar-type"));
    assert_eq!(
        performer.attributes[0].value.as_deref(),
        Some("electric guitar")
    );
    assert_eq!(
        performer.attributes[0].credited_as.as_deref(),
        Some("Fender Stratocaster")
    );
    assert!(found.facts.relationships_complete);
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
    let found = label_of_release(&response).expect("a label");
    assert_eq!(found.name, "Phonometrography");
    assert_eq!(found.mbid, None, "this answer named no identifier");
    assert!(label_of_release(&parse(r#"{"id":"x"}"#)).is_none());
}

#[test]
fn a_label_brings_its_musicbrainz_identifier_when_the_answer_carries_one() {
    let response = parse(
        r#"{"id":"59211ea4","title":"x","date":"2003-12-04",
            "label-info":[{"label":{"id":"c029628b","name":"Columbia"}}]}"#,
    );
    let found = label_of_release(&response).expect("a label");
    assert_eq!(found.name, "Columbia");
    assert_eq!(found.mbid.as_deref(), Some("c029628b"));
}

#[test]
fn the_name_and_the_identifier_never_come_from_two_different_labels() {
    // The first `label-info` entry names no label at all; the second does,
    // with an identifier. The name and the identifier read back must both
    // belong to that second entry — never the first entry's (absent) name
    // paired with the second entry's identifier, or vice versa.
    let response = parse(
        r#"{"id":"59211ea4","title":"x",
            "label-info":[
              {"catalog-number":"CAT-1"},
              {"label":{"id":"c029628b","name":"Columbia"}}
            ]}"#,
    );
    let found = label_of_release(&response).expect("a label");
    assert_eq!(found.name, "Columbia");
    assert_eq!(found.mbid.as_deref(), Some("c029628b"));
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

/// Two relations copied out of a live answer for **Ozzy Osbourne**
/// (`8aa5b65a-5b3c-4029-92bf-47a544356934`), `inc=artist-rels`, verbatim but
/// for the fields this program does not read.
///
/// A person, and the point of keeping it: his record holds **no** `member of
/// band` at all — a solo artist is not a band — and every musician who played
/// with him is an `instrumental supporting musician`. A parser that only knew
/// the obvious relationship would have shown him an empty line-up while
/// MusicBrainz plainly holds his band.
const OZZY_RELATIONS: &str = r#"{
  "id": "8aa5b65a-5b3c-4029-92bf-47a544356934",
  "name": "Ozzy Osbourne",
  "relations": [
    { "type": "instrumental supporting musician",
      "type-id": "ed6a7891-ce70-4e08-9839-1f2f62270497",
      "target-type": "artist",
      "direction": "backward",
      "begin": "1979-11",
      "end": "1982-03-19",
      "ended": true,
      "attributes": ["guitar"],
      "artist": { "type": "Person", "country": "US", "name": "Randy Rhoads",
                  "id": "19dccaac-efa8-413f-9042-28006792e0f2",
                  "sort-name": "Rhoads, Randy" } },
    { "type": "instrumental supporting musician",
      "type-id": "ed6a7891-ce70-4e08-9839-1f2f62270497",
      "target-type": "artist",
      "direction": "backward",
      "begin": "1979",
      "end": "1981",
      "ended": true,
      "attributes": ["bass guitar"],
      "artist": { "type": "Person", "country": "AU", "name": "Bob Daisley",
                  "id": "af49ecbb-a0b9-4805-9cec-f97eac794c81",
                  "sort-name": "Daisley, Bob" } }
  ]
}"#;

/// Two relations copied out of a live answer for **Judas Priest**
/// (`6b335658-22c8-485d-93de-0bc29a1d0349`), the same way.
///
/// A band this time, and it answers the question the two responses were fetched
/// to settle: the direction. Both a person's record and a band's say
/// `"backward"`, so backward cannot mean "this record is the person".
const PRIEST_RELATIONS: &str = r#"{
  "id": "6b335658-22c8-485d-93de-0bc29a1d0349",
  "name": "Judas Priest",
  "relations": [
    { "type": "instrumental supporting musician",
      "type-id": "ed6a7891-ce70-4e08-9839-1f2f62270497",
      "target-type": "artist",
      "direction": "backward",
      "begin": "2018",
      "end": null,
      "ended": false,
      "attributes": ["electric guitar"],
      "artist": { "type": "Person", "country": "GB", "name": "Andy Sneap",
                  "id": "2b9aed4d-769e-4c38-96e4-586ac59ce668",
                  "sort-name": "Sneap, Andy" } },
    { "type": "member of band",
      "type-id": "5be4c609-9afa-4ea0-910b-12ffb71e3821",
      "target-type": "artist",
      "direction": "backward",
      "begin": "1969",
      "end": "1970",
      "ended": true,
      "attributes": ["guitar family", "original"],
      "artist": { "type": "Person", "country": "GB", "name": "Ernie Chataway",
                  "id": "3651334d-6513-40e1-adbb-423acf0ad3d6",
                  "sort-name": "Chataway, Ernie" } }
  ]
}"#;

#[test]
fn a_backward_relation_names_the_player_whichever_record_it_came_from() {
    // **The question these two fixtures were fetched to answer.** One relation
    // is stated once and returned on both artists with a direction, and reading
    // it wrongly does not lose data — it inverts it. The first version of the
    // parser assumed a person's own record would read "forward"; both live
    // answers say "backward", so the direction is about the relationship's
    // definition (musician towards group) and not about which record you asked
    // for.
    for (who, json) in [("Ozzy", OZZY_RELATIONS), ("Judas Priest", PRIEST_RELATIONS)] {
        let found = artist(&parse(json)).expect("an artist");
        assert!(
            found.facts.bands().next().is_none(),
            "{who}: a backward relation is not a band this artist joined"
        );
        assert_eq!(
            found.facts.line_up().count(),
            2,
            "{who}: both relations name a player"
        );
    }
}

#[test]
fn a_solo_artist_has_supporting_musicians_and_no_members() {
    // Ozzy Osbourne's record holds not one `member of band`: he is a person,
    // not a group. Reading only the obvious relationship would have shown him
    // an empty line-up while MusicBrainz holds his whole band.
    let found = artist(&parse(OZZY_RELATIONS)).expect("an artist");
    let rows: Vec<&Membership> = found.facts.line_up().collect();
    assert!(
        rows.iter()
            .all(|m| m.kind == "instrumental supporting musician"),
        "{:?}",
        rows.iter().map(|m| &m.kind).collect::<Vec<_>>()
    );
    let randy = rows
        .iter()
        .find(|m| m.name == "Randy Rhoads")
        .expect("Randy");
    assert_eq!(randy.mbid, "19dccaac-efa8-413f-9042-28006792e0f2");
    assert_eq!(randy.attributes, vec!["guitar"]);
    // A partial date is a date: `1979-11` places him in 1980 as surely as
    // `1979` would, because only the year is ever compared.
    assert_eq!(randy.began.as_deref(), Some("1979-11"));
    assert_eq!(randy.covers(1980), Some(true));
    assert_eq!(randy.covers(1983), Some(false));
}

#[test]
fn an_attribute_is_not_always_an_instrument() {
    // Measured, and it is why the field is not called `instruments`: Judas
    // Priest's line-up carries `["guitar family", "original"]`, where
    // `original` marks an original member and is not an instrument at all.
    let found = artist(&parse(PRIEST_RELATIONS)).expect("an artist");
    let ernie = found
        .facts
        .line_up()
        .find(|m| m.name == "Ernie Chataway")
        .expect("Ernie");
    assert_eq!(ernie.kind, "member of band");
    assert_eq!(ernie.attributes, vec!["guitar family", "original"]);
    assert_eq!(ernie.years(), "1969–1970");
}

#[test]
fn a_null_end_beside_ended_false_is_a_current_member() {
    // The two fields arrive together — `"end": null, "ended": false` — and only
    // reading both says "still in the band" rather than "nobody filled it in".
    let found = artist(&parse(PRIEST_RELATIONS)).expect("an artist");
    let andy = found
        .facts
        .line_up()
        .find(|m| m.name == "Andy Sneap")
        .expect("Andy");
    assert_eq!(andy.ended, None);
    assert_eq!(andy.over, Some(false));
    assert_eq!(andy.years(), "2018–");
    assert_eq!(andy.covers(2024), Some(true));
}

#[test]
fn a_relationship_of_another_kind_is_not_a_membership() {
    // The list of relationships read is a filter, and everything else on an
    // artist's record — the URLs, a teacher, a marriage — must fall through it.
    let response = parse(
        r#"{"id":"x","name":"Someone","relations":[
            {"type":"teacher","direction":"backward",
             "artist":{"id":"t","name":"A Teacher"}},
            {"type":"wikidata","url":{"resource":"https://www.wikidata.org/wiki/Q1"}}]}"#,
    );
    let found = artist(&response).expect("an artist");
    assert!(found.facts.members.is_empty());
    assert_eq!(
        found.facts.wikidata.as_deref(),
        Some("https://www.wikidata.org/wiki/Q1"),
        "and the link relations still work"
    );
}
