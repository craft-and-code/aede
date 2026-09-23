use super::*;
use crate::args::Args;

fn expression(words: &[&str]) -> String {
    let args = Args::parse(words.iter().map(|w| w.to_string()));
    albums_query(&args).expect("a readable expression")
}

#[test]
fn an_album_listing_asks_for_the_album_artist_and_not_any_credit() {
    // The one mapping that is a decision rather than a transcription, and
    // the one no end-to-end test on this library could catch: the fixtures
    // hold nobody who guests on somebody else's album, so both spellings
    // answer the same there. The decision is therefore tested where it is
    // taken. Mapping `--artist` onto `artist:` would quietly list every
    // album an artist appears on as one of theirs.
    assert_eq!(
        expression(&["albums", "--artist", "Ozzy"]),
        "albumartist:Ozzy"
    );
    assert!(
        !expression(&["albums", "--artist", "Ozzy"]).starts_with("artist:"),
        "any credit is a different question from the album's own artist"
    );
}

#[test]
fn every_filter_option_becomes_one_term() {
    assert_eq!(expression(&["albums", "--genre", "metal"]), "genre:metal");
    assert_eq!(
        expression(&["albums", "--label", "Earache"]),
        "label:Earache"
    );
    assert_eq!(expression(&["albums", "--year", "1969"]), "year:1969");
    assert_eq!(
        expression(&["albums", "--comment", "vinyl"]),
        "comment:vinyl"
    );
    assert_eq!(
        expression(&["albums", "--compilations"]),
        "compilation:true"
    );
    assert_eq!(
        expression(&["albums", "--no-compilations"]),
        "compilation:false"
    );
    assert_eq!(expression(&["albums"]), "", "no filter is no expression");

    // A name with spaces has to survive being put into a query, or
    // `--artist Miles Davis` would become two terms and quietly ask for
    // albums whose artist is "Miles" *and* something called "Davis".
    assert_eq!(
        expression(&["albums", "--artist", "Miles Davis"]),
        "albumartist:\"Miles Davis\""
    );

    // Several options join with a space, which the grammar reads as AND.
    assert_eq!(
        expression(&["albums", "--genre", "metal", "--year", "1994"]),
        "year:1994 genre:metal"
    );
}

#[test]
fn a_year_that_is_not_one_is_refused_by_the_option_that_named_it() {
    let args = Args::parse(["albums", "--year", "abc"].iter().map(|w| w.to_string()));
    let error = albums_query(&args).expect_err("not a year");
    assert!(
        error.to_string().contains("--year expects a year"),
        "the message names the option typed, not the grammar: {error}"
    );
}
