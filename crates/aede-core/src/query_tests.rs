//! Tests for [`super`], split out of `query.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model;
use crate::user::{EntityRef, LOCAL_USER, Play, UserData};

fn catalog() -> Catalog {
    model::build(
        vec![
            model::tests::track(
                "/m/Deicide/Legion/01 Satan Spawn.flac",
                &[
                    ("title", "Satan Spawn"),
                    ("artist", "Deicide"),
                    ("albumartist", "Deicide"),
                    ("album", "Legion"),
                    ("date", "1992"),
                    ("genre", "Death Metal"),
                    ("label", "Roadrunner"),
                    ("comment", "vinyl rip"),
                ],
                200_000,
            ),
            model::tests::track(
                "/m/Ozzy/Blizzard/01 Crazy Train.flac",
                &[
                    ("title", "Crazy Train"),
                    ("artist", "Ozzy Osbourne"),
                    ("albumartist", "Ozzy Osbourne"),
                    ("album", "Blizzard of Ozz"),
                    ("date", "1980"),
                    ("genre", "Heavy Metal"),
                ],
                295_000,
            ),
            model::tests::track(
                "/m/Miles/Kind of Blue/01 So What.flac",
                &[
                    ("title", "So What"),
                    ("artist", "Miles Davis"),
                    ("albumartist", "Miles Davis"),
                    ("album", "Kind of Blue"),
                    ("date", "1959"),
                    ("genre", "Jazz"),
                ],
                545_000,
            ),
        ],
        vec!["/m".into()],
        0,
    )
}

fn titles(expression: &str, catalog: &Catalog, data: &UserData) -> Vec<String> {
    let query = parse(expression).unwrap_or_else(|e| panic!("{expression}: {e}"));
    let context = Context {
        catalog,
        data,
        owner: LOCAL_USER,
    };
    run(&query, &context)
        .into_iter()
        .filter_map(|id| catalog.track(id))
        .map(|t| t.title.clone())
        .collect()
}

#[test]
fn a_field_narrows_and_juxtaposition_means_and() {
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("genre:metal", &c, &d).len(), 2);
    assert_eq!(titles("genre:metal artist:ozzy", &c, &d), ["Crazy Train"]);
    assert!(titles("genre:metal artist:miles", &c, &d).is_empty());
}

#[test]
fn or_and_not_are_the_two_things_options_can_never_express() {
    // The whole reason for a grammar: `--genre a --genre b` can only ever
    // mean "and", and there is no spelling at all for "except".
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("artist:ozzy OR artist:miles", &c, &d).len(), 2);
    assert_eq!(titles("genre:metal -artist:ozzy", &c, &d), ["Satan Spawn"]);
    assert_eq!(titles("-genre:metal", &c, &d), ["So What"]);
    assert_eq!(
        titles("(artist:ozzy OR artist:deicide) year:..1985", &c, &d),
        ["Crazy Train"]
    );
}

#[test]
fn a_range_is_inclusive_and_either_end_may_be_left_open() {
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("year:1980..1992", &c, &d).len(), 2);
    assert_eq!(titles("year:..1959", &c, &d), ["So What"]);
    assert_eq!(titles("year:1992..", &c, &d), ["Satan Spawn"]);
    assert_eq!(titles("year:1980", &c, &d), ["Crazy Train"]);
    assert_eq!(titles("year:>=1980", &c, &d).len(), 2);
}

#[test]
fn a_track_with_nothing_to_compare_is_absent_rather_than_zero() {
    // Counting a missing year as zero would file every untagged file under
    // "before 1970" — an answer, and a wrong one.
    let c = model::build(
        vec![model::tests::track(
            "/m/x/a.flac",
            &[("title", "No Year"), ("artist", "A"), ("album", "B")],
            1000,
        )],
        vec!["/m".into()],
        0,
    );
    let d = UserData::default();
    assert!(titles("year:<2000", &c, &d).is_empty());
    assert!(titles("year:0..3000", &c, &d).is_empty());
    assert_eq!(titles("-year:<2000", &c, &d), ["No Year"]);
}

#[test]
fn what_the_user_wrote_is_queryable_and_says_where_it_was_written() {
    // "Rated five stars" is a different claim depending on whether the
    // stars were put on the track, the album or the artist, so the field
    // says which rather than folding the three together.
    let c = catalog();
    let mut d = UserData::default();
    let artist = EntityRef::new(EntityKind::Artist, "ozzy osbourne");
    d.entry(LOCAL_USER, &artist, 1).rating = Some(5);
    let track = EntityRef::new(EntityKind::Track, "/m/Miles/Kind of Blue/01 So What.flac");
    {
        let a = d.entry(LOCAL_USER, &track, 1);
        a.loved = true;
        a.tags.insert("vinyl".into());
        a.note = Some("the 1997 remaster".into());
    }

    assert_eq!(titles("artist.rating:5", &c, &d), ["Crazy Train"]);
    assert!(titles("rating:5", &c, &d).is_empty(), "not on the track");
    assert_eq!(titles("loved", &c, &d), ["So What"]);
    assert_eq!(titles("tag:vinyl", &c, &d), ["So What"]);
    assert_eq!(titles("note:remaster", &c, &d), ["So What"]);
    assert_eq!(titles("-loved", &c, &d).len(), 2);
}

#[test]
fn a_field_asked_by_its_name_alone_asks_whether_there_is_one() {
    // "Which things have I written a note on" had no way of being asked at
    // all: a bare `note` fell through to a text search for the word,
    // `note:true` searched for the word "true", and the fallback in
    // `flag_of` meant to answer exactly this was unreachable. The question
    // is the natural one to ask of anything the user writes.
    let c = catalog();
    let mut d = UserData::default();
    let track = EntityRef::new(EntityKind::Track, "/m/Miles/Kind of Blue/01 So What.flac");
    {
        let a = d.entry(LOCAL_USER, &track, 1);
        a.tags.insert("vinyl".into());
        a.note = Some("the 1997 remaster".into());
        a.rating = Some(4);
    }

    assert_eq!(titles("note", &c, &d), ["So What"]);
    assert_eq!(titles("tag", &c, &d), ["So What"]);
    assert_eq!(titles("rating", &c, &d), ["So What"]);
    // And its negation, which is how a library is combed for what has not
    // been annotated yet.
    assert_eq!(titles("-note", &c, &d).len(), 2);

    // The searches this could have broken still work: the two questions
    // were one predicate until it turned out they were two, and asking
    // "does it hold anything" must not cost the ability to ask "does it
    // hold *this*".
    assert_eq!(titles("note:remaster", &c, &d), ["So What"]);
    assert_eq!(titles("tag:vinyl", &c, &d), ["So What"]);
    assert_eq!(titles("rating:>=4", &c, &d), ["So What"]);
    assert!(titles("note:nineteen", &c, &d).is_empty());
}

#[test]
fn a_flag_may_be_asked_either_way_round() {
    // `lossless:false` reads better than `-lossless` and means the same;
    // accepting only one of the two makes the other a silent trap, since
    // a value on a flag field would otherwise match nothing at all.
    let c = catalog();
    let mut d = UserData::default();
    d.entry(
        LOCAL_USER,
        &EntityRef::new(EntityKind::Track, "/m/Miles/Kind of Blue/01 So What.flac"),
        1,
    )
    .loved = true;

    assert_eq!(titles("loved:true", &c, &d), ["So What"]);
    assert_eq!(titles("loved:false", &c, &d).len(), 2);
    assert_eq!(titles("lossless:yes", &c, &d).len(), 3);
    assert!(titles("lossless:no", &c, &d).is_empty());

    let error = parse("loved:banana").expect_err("neither yes nor no");
    assert!(error.message.contains("yes or a no"), "{error}");
}

#[test]
fn the_credit_table_can_be_asked_who_did_what() {
    // `artist:` matches any credit in any role, which is why two of them
    // already mean "both are on it". A role field asks the finer question
    // the graph was built for, and the one no pile of options expresses.
    let c = model::build(
        vec![
            model::tests::track(
                "/m/a/01.flac",
                &[
                    ("title", "Crazy Train"),
                    ("artist", "Ozzy Osbourne"),
                    ("album", "Blizzard"),
                    ("composer", "Randy Rhoads"),
                    ("producer", "Max Norman"),
                ],
                1000,
            ),
            model::tests::track(
                "/m/b/01.flac",
                &[
                    ("title", "Other"),
                    ("artist", "Randy Rhoads"),
                    ("album", "Elsewhere"),
                    ("composer", "Ozzy Osbourne"),
                ],
                1000,
            ),
        ],
        vec!["/m".into()],
        0,
    );
    let d = UserData::default();

    // The same two names, in swapped roles, are two different questions.
    assert_eq!(titles("composer:rhoads", &c, &d), ["Crazy Train"]);
    assert_eq!(titles("composer:ozzy", &c, &d), ["Other"]);
    assert_eq!(titles("producer:norman", &c, &d), ["Crazy Train"]);

    // And a role composes with everything else.
    assert_eq!(
        titles("composer:rhoads mainartist:ozzy", &c, &d),
        ["Crazy Train"]
    );
    assert!(titles("composer:rhoads mainartist:rhoads", &c, &d).is_empty());

    // `artist:` still means "credited at all, however".
    assert_eq!(titles("artist:ozzy", &c, &d).len(), 2);
}

#[test]
fn who_is_audible_is_its_own_question() {
    // Singing one guest verse counts; having written the words does not.
    // `artist --with` asks exactly this, and it is not the same as
    // "credited at all".
    let c = model::build(
        vec![model::tests::track(
            "/m/a/01.flac",
            &[
                ("title", "Crazy Train"),
                ("artist", "Ozzy Osbourne"),
                ("album", "Blizzard"),
                ("performer", "Randy Rhoads"),
                ("lyricist", "Bob Daisley"),
            ],
            1000,
        )],
        vec!["/m".into()],
        0,
    );
    let d = UserData::default();
    assert_eq!(titles("performing:rhoads", &c, &d), ["Crazy Train"]);
    assert_eq!(titles("performing:ozzy", &c, &d), ["Crazy Train"]);
    assert!(
        titles("performing:daisley", &c, &d).is_empty(),
        "writing the words is not being heard"
    );
    assert_eq!(
        titles("artist:daisley", &c, &d),
        ["Crazy Train"],
        "but he is credited all the same"
    );
}

#[test]
fn play_counts_answer_what_has_never_been_heard() {
    let c = catalog();
    let mut d = UserData::default();
    let track = EntityRef::new(EntityKind::Track, "/m/Ozzy/Blizzard/01 Crazy Train.flac");
    d.record_play(Play {
        owner: LOCAL_USER.into(),
        track,
        at: 1,
        ms_played: 1,
        completed: true,
    });
    assert_eq!(titles("played:>=1", &c, &d), ["Crazy Train"]);
    assert_eq!(titles("played:0", &c, &d).len(), 2);
}

#[test]
fn a_length_may_be_typed_the_way_it_is_read() {
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("duration:>5:00", &c, &d), ["So What"]);
    assert_eq!(titles("duration:..240", &c, &d), ["Satan Spawn"]);
}

#[test]
fn a_quoted_value_keeps_its_spaces_and_a_bare_word_searches() {
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("album:\"kind of blue\"", &c, &d), ["So What"]);
    assert_eq!(titles("crazy", &c, &d), ["Crazy Train"]);
    assert_eq!(titles("\"Miles Davis\"", &c, &d), ["So What"]);
}

#[test]
fn exact_and_contains_are_different_questions() {
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("album:legion", &c, &d), ["Satan Spawn"]);
    assert_eq!(titles("album:=legion", &c, &d), ["Satan Spawn"]);
    assert!(
        titles("album:=legio", &c, &d).is_empty(),
        "exact means exact"
    );
    assert_eq!(titles("album:legio", &c, &d), ["Satan Spawn"]);
}

#[test]
fn a_result_can_be_put_in_order_and_the_unknown_goes_last() {
    // "Unknown" is not "smallest": sorting by year must not open with
    // everything nobody ever tagged, whichever way round it is asked.
    let mut c = catalog();
    let d = UserData::default();
    let context = Context {
        catalog: &c,
        data: &d,
        owner: LOCAL_USER,
    };
    let mut tracks = run(&Query::All, &context);
    sort(&mut tracks, Sort::parse("year").unwrap(), &context);
    let years: Vec<u32> = tracks
        .iter()
        .filter_map(|&id| c.track(id))
        .filter_map(|t| t.release_id)
        .filter_map(|r| c.release(r))
        .filter_map(|r| r.year)
        .collect();
    assert_eq!(years, [1959, 1980, 1992]);

    let mut descending = run(&Query::All, &context);
    sort(&mut descending, Sort::parse("year-").unwrap(), &context);
    assert_eq!(
        c.track(descending[0]).map(|t| t.title.as_str()),
        Some("Satan Spawn")
    );

    // A track with no year lands last both ways.
    c = model::build(
        vec![
            model::tests::track(
                "/m/x/a.flac",
                &[("title", "No Year"), ("artist", "A"), ("album", "B")],
                1000,
            ),
            model::tests::track(
                "/m/y/b.flac",
                &[
                    ("title", "Dated"),
                    ("artist", "A"),
                    ("album", "C"),
                    ("date", "1999"),
                ],
                1000,
            ),
        ],
        vec!["/m".into()],
        0,
    );
    let context = Context {
        catalog: &c,
        data: &d,
        owner: LOCAL_USER,
    };
    for order in ["year", "year-"] {
        let mut tracks = run(&Query::All, &context);
        sort(&mut tracks, Sort::parse(order).unwrap(), &context);
        assert_eq!(
            c.track(*tracks.last().unwrap()).map(|t| t.title.as_str()),
            Some("No Year"),
            "sorted {order}"
        );
    }

    let error = Sort::parse("bananas").expect_err("not a key");
    assert!(
        error.message.contains("not something to sort on"),
        "{error}"
    );
    assert!(error.message.contains("year"), "and lists them: {error}");
}

#[test]
fn an_empty_query_matches_everything_and_a_broken_one_says_why() {
    let c = catalog();
    let d = UserData::default();
    assert_eq!(titles("", &c, &d).len(), 3);
    assert_eq!(titles("   ", &c, &d).len(), 3);

    assert!(parse("genre:").is_err(), "a field with nothing after it");
    assert!(parse("(genre:metal").is_err(), "an open bracket");
    assert!(parse("genre:metal)").is_err(), "a stray closing bracket");
    assert!(parse("\"unclosed").is_err(), "an open quotation mark");
    assert!(parse("year:abc").is_err(), "a year that is not one");

    // An unknown field names the ones that exist rather than shrugging.
    let error = parse("bogus:1").expect_err("unknown field");
    assert!(error.message.contains("not a field"), "{error}");
    assert!(error.message.contains("genre"), "{error}");
}
