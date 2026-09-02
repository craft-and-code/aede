//! Tests for [`super`], split out of `query.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::build;
use crate::model::tests::{example_catalog, first_release, track};

#[test]
fn the_shared_tracks_match_the_collaboration_weight() {
    // The weight of a `collaborated` relation is a count; the tracks it
    // counts must be reachable, or the graph cannot be walked.
    let c = example_catalog();
    let garou = c.find_artist("Garou").expect("Garou present");
    let celine = c.find_artist("Céline Dion").expect("Céline Dion present");
    let shared = c.tracks_in_common(garou.id, celine.id);
    let (_, weight, _) = c.neighbours_of_artist(garou.id)[0];
    assert_eq!(shared.len() as u32, weight, "count and list must agree");
    assert_eq!(
        c.track(shared[0]).map(|t| t.title.as_str()),
        Some("Sous le vent")
    );
    // A composer credit is not a collaboration, and neither is being alone.
    assert!(c.tracks_in_common(garou.id, garou.id).len() == 1);
}

#[test]
fn an_exact_album_title_wins_over_the_ones_beginning_with_it() {
    // "Danzig" used to match "Danzig 4" and return whichever came first in
    // the catalog: an arbitrary answer, given without saying so.
    let album = |folder: &str, title: &'static str| {
        track(
            &format!("{folder}/01.flac"),
            &[
                ("title", "A song"),
                ("artist", "Danzig"),
                ("albumartist", "Danzig"),
                ("album", title),
            ],
            120_000,
        )
    };
    let c = build(
        vec![
            album("/m/4", "Danzig 4"),
            album("/m/1", "Danzig"),
            album("/m/2", "Danzig II: Lucifuge"),
        ],
        vec!["/m".into()],
        0,
    );

    let (exact, kind) = c.find_releases("Danzig");
    assert_eq!(kind, TitleMatch::Exact);
    assert_eq!(exact.len(), 1, "one album is titled exactly that");
    assert_eq!(exact[0].title, "Danzig");
    assert_eq!(
        first_release(&c, "Danzig").map(|r| r.title.as_str()),
        Some("Danzig")
    );

    // With no exact title, every match is returned rather than one of them.
    let (partial, kind) = c.find_releases("danzig i");
    assert_eq!(kind, TitleMatch::Partial);
    assert_eq!(partial.len(), 1);
    // Normalization trims the query, so a trailing space is not a way to
    // ask for "the longer titles"; a fragment is.
    let (several, kind) = c.find_releases("anzig");
    assert_eq!(kind, TitleMatch::Partial);
    assert_eq!(several.len(), 3, "every title containing it");
}

#[test]
fn every_track_sharing_a_title_is_returned() {
    // The studio version and the live one are two different recordings
    // that happen to share a name; neither may hide the other.
    let c = build(
        vec![
            track(
                "/m/a/01 Ride the Lightning.flac",
                &[
                    ("title", "Ride the Lightning"),
                    ("artist", "Metallica"),
                    ("album", "Ride the Lightning"),
                ],
                120_000,
            ),
            track(
                "/m/b/03 Ride the Lightning.flac",
                &[
                    ("title", "Ride the Lightning"),
                    ("artist", "Metallica"),
                    ("album", "Live Shit"),
                ],
                400_000,
            ),
        ],
        vec!["/m".to_string()],
        0,
    );
    let (found, kind) = c.find_tracks("ride the lightning");
    assert_eq!(kind, TitleMatch::Exact, "the title matches as written");
    assert_eq!(found.len(), 2, "both recordings are returned");
    let durations: Vec<Option<u64>> = found.iter().map(|t| t.duration_ms).collect();
    assert!(durations.contains(&Some(400_000)), "the live one is there");
}

#[test]
fn a_partial_title_widens_the_search_only_as_a_last_resort() {
    let c = example_catalog();
    let (exact, kind) = c.find_tracks("Fight Fire with Fire");
    assert_eq!(kind, TitleMatch::Exact);
    assert_eq!(exact.len(), 1);

    let (partial, kind) = c.find_tracks("fight fire");
    assert_eq!(kind, TitleMatch::Partial, "no title is exactly that");
    assert_eq!(partial.len(), 1);

    let (nothing, _) = c.find_tracks("nothing of the sort");
    assert!(nothing.is_empty());
}

#[test]
fn tracks_are_ordered_within_the_release() {
    let c = example_catalog();
    let album = first_release(&c, "Ride the Lightning").expect("album found");
    let titles: Vec<&str> = album
        .track_ids
        .iter()
        .map(|&id| c.track(id).unwrap().title.as_str())
        .collect();
    assert_eq!(titles, ["Fight Fire with Fire", "Ride the Lightning"]);
    assert_eq!(album.year, Some(1984));
}

#[test]
fn a_guest_appearance_is_not_part_of_the_discography() {
    // Exactly the shape that made "The Sinister Urge" show up under Ozzy
    // Osbourne: he sings one track on a Rob Zombie album.
    let c = build(
        vec![
            track(
                "/m/Rob Zombie/The Sinister Urge/04 Never Gonna Stop.flac",
                &[
                    ("title", "Never Gonna Stop"),
                    ("artist", "Rob Zombie"),
                    ("albumartist", "Rob Zombie"),
                    ("album", "The Sinister Urge"),
                    ("date", "2001"),
                ],
                60_000,
            ),
            track(
                "/m/Rob Zombie/The Sinister Urge/05 Iron Head.flac",
                &[
                    ("title", "Iron Head"),
                    ("artist", "Rob Zombie"),
                    ("performer", "Ozzy Osbourne"),
                    ("albumartist", "Rob Zombie"),
                    ("album", "The Sinister Urge"),
                    ("date", "2001"),
                ],
                60_000,
            ),
            track(
                "/m/Ozzy Osbourne/Blizzard of Ozz/01 I Dont Know.flac",
                &[
                    ("title", "I Don't Know"),
                    ("artist", "Ozzy Osbourne"),
                    ("albumartist", "Ozzy Osbourne"),
                    ("album", "Blizzard of Ozz"),
                    ("date", "1980"),
                ],
                60_000,
            ),
        ],
        vec!["/m".into()],
        0,
    );

    let ozzy = c
        .find_artist("Ozzy Osbourne")
        .expect("Ozzy is in the catalog");
    let own: Vec<&str> = c
        .releases_as_album_artist(ozzy.id)
        .iter()
        .filter_map(|&id| c.release(id))
        .map(|r| r.title.as_str())
        .collect();
    assert_eq!(
        own,
        ["Blizzard of Ozz"],
        "the discography holds his own albums only"
    );

    let guest: Vec<&str> = c
        .guest_appearances(ozzy.id)
        .iter()
        .filter_map(|&id| c.release(id))
        .map(|r| r.title.as_str())
        .collect();
    assert_eq!(
        guest,
        ["The Sinister Urge"],
        "the guest album is listed apart"
    );

    // The old, undifferentiated view still returns both.
    assert_eq!(c.releases_of_artist(ozzy.id).len(), 2);
}

#[test]
fn a_writing_credit_is_neither_discography_nor_appearance() {
    let c = build(
        vec![track(
            "/m/Ozzy Osbourne/Blizzard of Ozz/01 Crazy Train.flac",
            &[
                ("title", "Crazy Train"),
                ("artist", "Ozzy Osbourne"),
                ("albumartist", "Ozzy Osbourne"),
                ("album", "Blizzard of Ozz"),
                ("composer", "Randy Rhoads"),
                ("date", "1980"),
            ],
            60_000,
        )],
        vec!["/m".into()],
        0,
    );

    let rhoads = c
        .find_artist("Randy Rhoads")
        .expect("the composer is an entity");
    assert!(c.releases_as_album_artist(rhoads.id).is_empty());
    assert!(c.guest_appearances(rhoads.id).is_empty());
    assert_eq!(c.releases_written_without_performing(rhoads.id).len(), 1);
    assert!(c.performed_tracks_of_artist(rhoads.id).is_empty());
}

#[test]
fn what_someone_wrote_is_counted_even_when_they_play_it_too() {
    // Ozzy Osbourne, sixty-nine composer credits and sixty-eight as
    // lyricist, was announced on his own page as writing one track: the
    // figure reported the size of a display table, which leaves out
    // everything he also sings on. A number that answers a narrower
    // question than its label is worse than no number.
    let c = build(
        vec![track(
            "/m/Ozzy Osbourne/Blizzard of Ozz/01 Crazy Train.flac",
            &[
                ("title", "Crazy Train"),
                ("artist", "Ozzy Osbourne"),
                ("albumartist", "Ozzy Osbourne"),
                ("album", "Blizzard of Ozz"),
                ("composer", "Ozzy Osbourne"),
                ("date", "1980"),
            ],
            60_000,
        )],
        vec!["/m".into()],
        0,
    );
    let ozzy = c.find_artist("Ozzy Osbourne").expect("the artist");

    // He sings it and he wrote it: both are true, and both are counted.
    assert_eq!(c.performed_tracks_of_artist(ozzy.id).len(), 1);
    assert_eq!(
        c.writing_tracks_of_artist(ozzy.id).len(),
        1,
        "writing counts what he wrote, not what he wrote and does not play"
    );
    assert_eq!(c.releases_with_writing_credit(ozzy.id).len(), 1);

    // The display set is the one that subtracts, and it is empty here:
    // the album is already in the discography above it.
    assert!(c.written_tracks_without_performing(ozzy.id).is_empty());
    assert!(c.releases_written_without_performing(ozzy.id).is_empty());

    // The property that makes the page consistent: every non-performing
    // credit the artist holds on a track is in the writing set. This is
    // what the summary line and the Roles panel are both read from, so
    // they cannot contradict one another again.
    let written: std::collections::BTreeSet<Id> =
        c.writing_tracks_of_artist(ozzy.id).into_iter().collect();
    for credit in c.credits.iter().filter(|x| x.artist_id == ozzy.id) {
        if credit.entity_kind == EntityKind::Track && !is_performing_role(&credit.role) {
            assert!(
                written.contains(&credit.entity_id),
                "a {} credit is missing from the writing set",
                credit.role
            );
        }
    }
}

#[test]
fn navigation_from_artist_to_albums() {
    let c = example_catalog();
    let metallica = c.find_artist("Metallica").unwrap();
    let albums = c.releases_of_artist(metallica.id);
    assert_eq!(albums.len(), 1);
    assert_eq!(c.release(albums[0]).unwrap().title, "Ride the Lightning");
}

#[test]
fn search_is_accent_insensitive() {
    let c = example_catalog();
    let hits = c.search("celine", 10);
    assert!(
        hits.iter().any(|h| h.name == "Céline Dion"),
        "got: {hits:?}"
    );
}

#[test]
fn genres_attached_to_track_and_album() {
    let c = example_catalog();
    let album = first_release(&c, "Ride the Lightning").unwrap();
    let genres = c.genres_of(EntityKind::Release, album.id);
    assert_eq!(genres.len(), 1);
    assert_eq!(genres[0].name, "Thrash Metal");
}
