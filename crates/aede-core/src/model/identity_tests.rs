//! Tests for [`super`], split out of `identity.rs`.

use super::*;

fn said(pairs: &[(&str, &str)]) -> Aliases {
    with_choices(pairs, &[])
}

/// The same, plus what the owner of the disk typed.
fn with_choices(pairs: &[(&str, &str)], chosen: &[(&str, &str)]) -> Aliases {
    let chosen: Vec<Chosen> = chosen
        .iter()
        .map(|(spelling, filed_as)| Chosen {
            spelling: crate::text::normalize(spelling),
            filed_as: crate::text::normalize(filed_as),
        })
        .collect();
    aliases(
        pairs.iter().map(|(mbid, name)| Said {
            mbid: mbid.to_string(),
            name: name.to_string(),
        }),
        &chosen,
    )
}

#[test]
fn two_spellings_under_one_identifier_are_one_artist() {
    // The half of artist identity that needs no heuristic: not a resemblance
    // to be judged, but the same artist, said so by the only authority there
    // is on the question.
    let found = said(&[
        ("ozzy-mbid", "Ozzy Osbourne"),
        ("ozzy-mbid", "Ozzy Osbourne"),
        ("ozzy-mbid", "O. Osbourne"),
    ]);
    // Both sides of the table are normalised names: the full stop is not part
    // of a matching key here any more than it is anywhere else in the program.
    assert_eq!(crate::text::normalize("O. Osbourne"), "o osbourne");
    assert_eq!(filed_as(&found, "o osbourne"), "ozzy osbourne");
    assert_eq!(
        filed_as(&found, "ozzy osbourne"),
        "ozzy osbourne",
        "the surviving spelling is filed under itself"
    );
}

#[test]
fn the_spelling_that_names_the_most_tracks_survives() {
    // Derived from the library rather than chosen: the shelf's own most
    // frequent way of writing the name. And it is deliberately a key that
    // already existed, so merging can only ever shrink the set of keys and
    // anything filed under the survivor in user.json keeps pointing at it.
    let found = said(&[
        ("mbid", "O. Osbourne"),
        ("mbid", "O. Osbourne"),
        ("mbid", "O. Osbourne"),
        ("mbid", "Ozzy Osbourne"),
    ]);
    assert_eq!(
        filed_as(&found, "ozzy osbourne"),
        "o osbourne",
        "the count decides, not which reads better to a human"
    );
}

#[test]
fn a_tie_is_broken_the_same_way_on_every_run() {
    // A `HashMap` promises no order, and a catalog that named an artist
    // differently on Tuesday would be unusable — a listing nobody can diff and
    // an entity key that moves under `user.json`.
    let pairs = [("mbid", "Zed Smith"), ("mbid", "Al Smith")];
    let first = said(&pairs);
    for _ in 0..20 {
        assert_eq!(said(&pairs), first, "the same library, the same answer");
    }
    assert_eq!(filed_as(&first, "zed smith"), "al smith");
}

#[test]
fn one_spelling_is_nobody_s_alias() {
    // Mapping a name to itself would make every lookup that misses look like
    // one that hit, and would grow a table with a row per artist in the
    // library for no purpose.
    let found = said(&[("mbid", "Nirvana"), ("mbid", "Nirvana")]);
    assert!(found.is_empty());
    assert_eq!(filed_as(&found, "nirvana"), "nirvana");
}

#[test]
fn two_identifiers_are_two_artists_however_alike_the_names() {
    // The fault the whole approach exists to avoid: matching on a fragment of
    // a name merges Angus Young with Neil Young. Two identifiers say they are
    // two people, and that is the end of it.
    let found = said(&[("angus", "Angus Young"), ("neil", "Neil Young")]);
    assert!(found.is_empty());
    assert_eq!(filed_as(&found, "angus young"), "angus young");
    assert_eq!(filed_as(&found, "neil young"), "neil young");
}

#[test]
fn an_empty_identifier_or_name_says_nothing() {
    // A tag present and blank is not evidence, and treating it as one would
    // file every unnamed artist in the library under a single key.
    let found = said(&[
        ("", "Ozzy Osbourne"),
        ("", "O. Osbourne"),
        ("mbid", "   "),
        ("mbid", "Ozzy Osbourne"),
    ]);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn spelling_differences_normalize_away_before_they_are_counted() {
    // `Ozzy  Osbourne` and `ozzy osbourne` are already one key everywhere else
    // in this program, and counting them apart here would let a difference
    // that does not exist decide which spelling wins.
    let found = said(&[
        ("mbid", "Ozzy  Osbourne"),
        ("mbid", "ozzy osbourne"),
        ("mbid", "O. Osbourne"),
    ]);
    assert_eq!(filed_as(&found, "o osbourne"), "ozzy osbourne");
}

#[test]
fn a_person_can_say_what_no_identifier_can() {
    // The other half of the problem, and the whole reason `aede merge` exists:
    // files that never met MusicBrainz carry nothing to merge them on, and
    // nobody outside the owner of the disk knows the answer.
    let found = with_choices(&[], &[("O. Osbourne", "Ozzy Osbourne")]);
    assert_eq!(filed_as(&found, "o osbourne"), "ozzy osbourne");
    assert_eq!(filed_as(&found, "ozzy osbourne"), "ozzy osbourne");
}

#[test]
fn what_a_person_said_beats_what_the_counts_would_have_chosen() {
    // The files elect `o osbourne` — it names three tracks against one — and
    // the person says the opposite. Theirs is the answer: the count is a
    // default for when nobody has spoken, not a rival authority.
    let pairs = [
        ("mbid", "O. Osbourne"),
        ("mbid", "O. Osbourne"),
        ("mbid", "O. Osbourne"),
        ("mbid", "Ozzy Osbourne"),
    ];
    assert_eq!(
        filed_as(&said(&pairs), "ozzy osbourne"),
        "o osbourne",
        "with nobody saying otherwise, the count decides"
    );
    let found = with_choices(&pairs, &[("O. Osbourne", "Ozzy Osbourne")]);
    assert_eq!(filed_as(&found, "o osbourne"), "ozzy osbourne");
    assert_eq!(
        filed_as(&found, "ozzy osbourne"),
        "ozzy osbourne",
        "and the row they named is nobody's alias, or the two point at each other"
    );
}

#[test]
fn two_statements_in_a_chain_land_everybody_on_the_last_one() {
    // `A → B` and `B → C`, typed on two different days, neither knowing about
    // the other. Resolving them as pairs one after another would leave `A`
    // pointing at a name that is itself an alias — a table one lookup cannot
    // read. They are one group with one survivor instead.
    let found = with_choices(
        &[],
        &[("O. Osbourne", "Ozzy O."), ("Ozzy O.", "Ozzy Osbourne")],
    );
    assert_eq!(filed_as(&found, "o osbourne"), "ozzy osbourne");
    assert_eq!(filed_as(&found, "ozzy o"), "ozzy osbourne");
    assert_eq!(filed_as(&found, "ozzy osbourne"), "ozzy osbourne");
}

#[test]
fn a_person_contradicting_themselves_still_gets_one_artist() {
    // `A → B` and `B → A`. Applied as edges over a table this loops for ever
    // and the shelf renames an artist on every scan. Nobody is elected by
    // name, the count decides, and the part they did agree on — that the two
    // are one musician — holds.
    let found = with_choices(
        &[("mbid", "Zed Smith"), ("mbid", "Al Smith")],
        &[("Zed Smith", "Al Smith"), ("Al Smith", "Zed Smith")],
    );
    let one = filed_as(&found, "zed smith");
    let other = filed_as(&found, "al smith");
    assert_eq!(one, other, "one artist, whichever spelling survived");
    assert!(found.len() == 1, "and exactly one of them is an alias");
}

#[test]
fn a_statement_joins_two_identifiers_worth_of_spellings() {
    // The files know `ozzy osbourne` and `o osbourne` are one man; the person
    // adds a third spelling from a folder that never met Picard. All three end
    // up under one name, which is what makes a merge worth stating at all.
    let found = with_choices(
        &[
            ("ozzy-mbid", "Ozzy Osbourne"),
            ("ozzy-mbid", "Ozzy Osbourne"),
            ("ozzy-mbid", "O. Osbourne"),
        ],
        &[("Ozzy", "Ozzy Osbourne")],
    );
    for spelling in ["o osbourne", "ozzy"] {
        assert_eq!(filed_as(&found, spelling), "ozzy osbourne", "{spelling}");
    }
}

#[test]
fn a_statement_that_says_nothing_is_ignored() {
    // A name filed under itself, and a half-written row. Neither is a
    // statement, and treating either as one would file every artist in the
    // library under the empty key.
    let found = with_choices(
        &[],
        &[("Ozzy Osbourne", "Ozzy Osbourne"), ("Ozzy Osbourne", "")],
    );
    assert!(found.is_empty(), "{found:?}");
}
