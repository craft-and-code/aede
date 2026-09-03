//! Tests for [`super`], split out of `places.rs`.
//!
//! Built on a real catalog and a real layer rather than on hand-made structs:
//! the whole point of this module is that it reaches across the two, and a
//! test that skipped one of them would prove nothing about the crossing.

use super::*;
use crate::model::builder::{ScannedFile, build};
use crate::sources::{ArtistFacts, Confidence, SourceRecord, Sources};
use crate::tags::RawTags;

/// A catalog of one track per artist named.
fn library(artists: &[&str]) -> Catalog {
    let files = artists
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let mut tags = RawTags::default();
            tags.insert("artist", *name);
            tags.insert("albumartist", *name);
            tags.insert("album", "An album");
            tags.insert("title", "A track");
            ScannedFile {
                path: format!("/music/{name}/01.flac"),
                size: n as u64 + 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
            }
        })
        .collect();
    build(files, vec!["/music".to_string()], 1)
}

/// Files a country against an artist, the way a fetch would.
fn says(held: &mut Sources, catalog: &Catalog, name: &str, area: Option<&str>) {
    says_with_code(held, catalog, name, area, None)
}

/// The same, with the ISO code the source states beside the name.
fn says_with_code(
    held: &mut Sources,
    catalog: &Catalog,
    name: &str,
    area: Option<&str>,
    code: Option<&str>,
) {
    let artist = catalog
        .artists
        .iter()
        .find(|a| a.name == name)
        .expect("an artist by that name");
    let entity = EntityRef::of(catalog, EntityKind::Artist, artist.id).expect("a reference");
    held.set(SourceRecord {
        key: entity.key,
        source: crate::sources::MUSICBRAINZ.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            area: area.map(str::to_string),
            country_code: code.map(str::to_string),
            ..Default::default()
        }),
    });
}

#[test]
fn a_country_is_read_out_of_the_layer_and_not_out_of_the_catalog() {
    let catalog = library(&["Air", "Miles Davis"]);
    let mut held = Sources::default();
    says(&mut held, &catalog, "Air", Some("France"));

    let air = catalog
        .artists
        .iter()
        .find(|a| a.name == "Air")
        .expect("Air");
    let miles = catalog
        .artists
        .iter()
        .find(|a| a.name == "Miles Davis")
        .expect("Miles");
    assert_eq!(
        country_of(&catalog, &held, air.id),
        Some("France".to_string())
    );
    assert_eq!(
        country_of(&catalog, &held, miles.id),
        None,
        "nobody asked about him, and that is not the same as being from nowhere"
    );
}

#[test]
fn an_empty_area_is_a_silence_and_not_a_country() {
    // MusicBrainz answers with the field present and blank often enough that
    // a library would otherwise grow a country whose name is nothing at all,
    // sitting in the listing under an empty row.
    let catalog = library(&["Air"]);
    let mut held = Sources::default();
    says(&mut held, &catalog, "Air", Some("   "));
    let air = catalog
        .artists
        .iter()
        .find(|a| a.name == "Air")
        .expect("Air");
    assert_eq!(country_of(&catalog, &held, air.id), None);
    assert!(countries(&catalog, &held).is_empty());
}

#[test]
fn the_listing_is_ranked_and_ties_are_broken_the_same_way_twice() {
    // A listing that shuffles itself between two runs over an unchanged
    // library cannot be diffed, and this one is meant to be read twice.
    let catalog = library(&["Air", "Daft Punk", "Miles Davis", "Portishead"]);
    let mut held = Sources::default();
    says(&mut held, &catalog, "Air", Some("France"));
    says(&mut held, &catalog, "Daft Punk", Some("France"));
    says(&mut held, &catalog, "Miles Davis", Some("United States"));
    says(&mut held, &catalog, "Portishead", Some("United Kingdom"));

    let found = countries(&catalog, &held);
    assert_eq!(
        found.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["France", "United Kingdom", "United States"],
        "two artists first, then the ones with one apiece in name order"
    );
    assert_eq!(found[0].artists.len(), 2);
    assert_eq!(countries(&catalog, &held), found, "and again, identically");
}

#[test]
fn a_name_matches_exactly_before_it_matches_partly() {
    let catalog = library(&["Air", "Miles Davis", "Portishead"]);
    let mut held = Sources::default();
    says(&mut held, &catalog, "Air", Some("France"));
    says(&mut held, &catalog, "Miles Davis", Some("United States"));
    says(&mut held, &catalog, "Portishead", Some("United Kingdom"));
    let all = countries(&catalog, &held);

    let (exact, how) = find(&all, "France");
    assert_eq!(how, TitleMatch::Exact);
    assert_eq!(exact.len(), 1);

    // Case and accents go through `normalize`, like every other facet.
    assert_eq!(find(&all, "  fRaNce ").0.len(), 1);

    // One word, two countries — and the caller is told it was a partial match
    // so the answer can say so rather than pretend one was asked for.
    let (partial, how) = find(&all, "united");
    assert_eq!(how, TitleMatch::Partial);
    assert_eq!(partial.len(), 2);

    assert!(find(&all, "Belgium").0.is_empty());
    assert!(find(&all, "").0.is_empty());
}

#[test]
fn how_many_artists_were_asked_about_is_a_different_count_from_how_many_answered() {
    // The denominator behind every message: "no country matches" means one
    // thing in a library that has been fetched and quite another in one that
    // has not.
    let catalog = library(&["Air", "Miles Davis", "Portishead"]);
    let mut held = Sources::default();
    says(&mut held, &catalog, "Air", Some("France"));
    // Asked, and MusicBrainz had no area for him.
    says(&mut held, &catalog, "Miles Davis", None);

    assert_eq!(asked_about(&catalog, &held), 2);
    assert_eq!(countries(&catalog, &held).len(), 1);
    assert_eq!(
        asked_about(&catalog, &Sources::default()),
        0,
        "a library that has never fetched has no countries and no question asked"
    );
}

#[test]
fn a_short_form_is_derived_and_never_invented() {
    // The whole design decision. A table of synonyms — UK, GB, Great Britain,
    // Royaume-Uni — is a slope with no bottom: whose vernacular, in which
    // language, maintained by whom. So every short form here comes from the
    // source's own code or from the name itself.
    let catalog = library(&["Air", "Portishead", "Miles Davis"]);
    let mut held = Sources::default();
    says_with_code(&mut held, &catalog, "Air", Some("France"), Some("FR"));
    says_with_code(
        &mut held,
        &catalog,
        "Portishead",
        Some("United Kingdom"),
        Some("GB"),
    );
    says_with_code(
        &mut held,
        &catalog,
        "Miles Davis",
        Some("United States"),
        Some("US"),
    );
    let all = countries(&catalog, &held);

    let uk = all
        .iter()
        .find(|p| p.name == "United Kingdom")
        .expect("the United Kingdom");
    assert_eq!(uk.initials().as_deref(), Some("uk"));
    assert_eq!(uk.code.as_deref(), Some("gb"));
    assert_eq!(
        uk.short_forms(),
        vec!["GB".to_string(), "UK".to_string()],
        "the code an authority assigns first, then the initials"
    );

    // A one-word country has no initials: a single letter would match half
    // the world, so France is reachable by its code and its name alone.
    let france = all.iter().find(|p| p.name == "France").expect("France");
    assert_eq!(france.initials(), None);
    assert_eq!(france.short_forms(), vec!["FR".to_string()]);

    // Where the code and the initials are the same word, it is offered once:
    // printing "US US" would look like two different things.
    let us = all
        .iter()
        .find(|p| p.name == "United States")
        .expect("the United States");
    assert_eq!(us.short_forms(), vec!["US".to_string()]);
}

#[test]
fn a_code_and_a_set_of_initials_are_identifiers_and_not_guesses() {
    let catalog = library(&["Air", "Portishead", "Miles Davis"]);
    let mut held = Sources::default();
    says_with_code(&mut held, &catalog, "Air", Some("France"), Some("FR"));
    says_with_code(
        &mut held,
        &catalog,
        "Portishead",
        Some("United Kingdom"),
        Some("GB"),
    );
    says_with_code(
        &mut held,
        &catalog,
        "Miles Davis",
        Some("United States"),
        Some("US"),
    );
    let all = countries(&catalog, &held);

    for (typed, expected) in [
        ("united kingdom", "United Kingdom"),
        ("GB", "United Kingdom"),
        ("uk", "United Kingdom"),
        ("fr", "France"),
        ("us", "United States"),
    ] {
        let (found, how) = find(&all, typed);
        assert_eq!(found.len(), 1, "\"{typed}\" reaches one country");
        assert_eq!(found[0].name, expected, "typed: {typed}");
        assert_eq!(
            how,
            TitleMatch::Exact,
            "\"{typed}\" names exactly one country, and telling the reader it \
             only partly matched would be false modesty about a certainty"
        );
    }

    // Only the last step widens, and it is reported as what it is.
    let (found, how) = find(&all, "united");
    assert_eq!(found.len(), 2);
    assert_eq!(how, TitleMatch::Partial);

    // Neither the source's name nor its code, and this program speaks English:
    // refused, so that the error can name the listing that does work.
    for refused in ["USA", "Royaume-Uni", "Great Britain", "England"] {
        assert!(find(&all, refused).0.is_empty(), "refused: {refused}");
    }
}

#[test]
fn one_artist_fetched_since_the_code_was_kept_gives_the_whole_country_its_code() {
    // The code is a property of the place, not of whoever happened to be
    // asked about first — and a library part-fetched before this field
    // existed is the ordinary case, not an edge one.
    let catalog = library(&["Air", "Daft Punk"]);
    let mut held = Sources::default();
    says(&mut held, &catalog, "Air", Some("France"));
    says_with_code(&mut held, &catalog, "Daft Punk", Some("France"), Some("FR"));

    let all = countries(&catalog, &held);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].code.as_deref(), Some("fr"));
    assert_eq!(find(&all, "fr").0.len(), 1);

    // And with nobody fetched since, the country still works by name — it
    // simply offers no code, which is honest rather than broken.
    let mut older = Sources::default();
    says(&mut older, &catalog, "Air", Some("France"));
    let all = countries(&catalog, &older);
    assert_eq!(all[0].code, None);
    assert!(
        all[0].short_forms().is_empty(),
        "it offers only what it can actually match"
    );
    assert_eq!(find(&all, "france").0.len(), 1);

    // `fr` still reaches France, by the last step rather than the second —
    // and the difference is reported. A code is a certainty; a substring is
    // a widening that happens to land on one country today and may land on
    // two tomorrow.
    let (found, how) = find(&all, "fr");
    assert_eq!(found.len(), 1);
    assert_eq!(how, TitleMatch::Partial, "not offered, merely reached");
}
