//! Tests for [`super`], split out of `builder.rs`.
//!
//! Declared there with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//! Only the length of a file changed.

use super::*;
use crate::model::tests::{example_catalog, first_release, track};

#[test]
fn entities_are_deduplicated() {
    let c = example_catalog();
    // Metallica appears only once despite two tracks.
    assert_eq!(
        c.artists.iter().filter(|a| a.name == "Metallica").count(),
        1
    );
    assert_eq!(c.releases.len(), 2);
    assert_eq!(c.tracks.len(), 3);
}

#[test]
fn featuring_creates_two_artists_and_one_link() {
    let c = example_catalog();
    let garou = c.find_artist("Garou").expect("Garou present");
    let celine = c.find_artist("Céline Dion").expect("Céline Dion present");
    let neighbours = c.neighbours_of_artist(garou.id);
    assert_eq!(neighbours.len(), 1);
    assert_eq!(neighbours[0].0.id, celine.id);
    assert_eq!(neighbours[0].1, 1, "one track in common");
    // The link is indeed symmetric.
    assert_eq!(c.neighbours_of_artist(celine.id)[0].0.id, garou.id);
}

#[test]
fn various_artists_is_not_an_artist() {
    let c = build(
        vec![track(
            "/m/Various/Hits/01 Song.flac",
            &[
                ("title", "Song"),
                ("artist", "Performer"),
                ("album", "Hits"),
                ("albumartist", "Various Artists"),
            ],
            60_000,
        )],
        vec!["/m".into()],
        0,
    );
    assert!(c.find_artist("Various Artists").is_none());
    assert_eq!(c.artists.len(), 1, "only the performer must exist");
    let hits = first_release(&c, "Hits").unwrap();
    assert!(hits.is_compilation);
    assert_eq!(hits.album_artist_id, None);
}

#[test]
fn a_box_set_in_disc_folders_is_one_album() {
    // The layout every box set and every game soundtrack uses:
    // `Album/Disc 1`, `Album/Disc 2`. Keyed on the folder they landed in,
    // one release became two of the same name, each numbering its tracks
    // from one — and nothing on screen said which disc was which except
    // the path. A disc folder is a subdivision of a release, not another
    // edition of it.
    let mut files = Vec::new();
    for disc in 1..=2 {
        for track in 1..=2 {
            let mut tags = RawTags::default();
            tags.insert("title", format!("D{disc} T{track}"));
            tags.insert("artist", "Nobuo Uematsu");
            tags.insert("albumartist", "Nobuo Uematsu");
            tags.insert("album", "FINAL FANTASY VII");
            tags.insert("date", "1997");
            tags.insert("tracknumber", track.to_string());
            tags.insert("discnumber", disc.to_string());
            files.push(ScannedFile {
                path: format!("/m/Uematsu/FF7/Disc {disc}/{track:02}.flac"),
                size: 100,
                mtime: 0,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
            });
        }
    }
    let c = build(files, vec!["/m".into()], 0);
    assert_eq!(c.releases.len(), 1, "one album, not one per disc");
    assert_eq!(c.releases[0].track_ids.len(), 4);
    // The release lives where the album does, not in one of its discs —
    // which is also what `copy` reproduces and what `doctor` names.
    assert_eq!(c.releases[0].folder, "/m/Uematsu/FF7");
    let discs: Vec<Option<u32>> = c.tracks.iter().map(|t| t.disc_no).collect();
    assert_eq!(discs, vec![Some(1), Some(1), Some(2), Some(2)]);
}

#[test]
fn a_disc_folder_supplies_the_number_the_tags_forgot() {
    // A rip that split the discs into folders and left `discnumber` empty
    // is common. Merged without this, both discs would be disc one and the
    // release would hold two track 1s — worse than the split it replaces.
    let mut files = Vec::new();
    for disc in 1..=2 {
        let mut tags = RawTags::default();
        tags.insert("title", format!("D{disc}"));
        tags.insert("artist", "A");
        tags.insert("albumartist", "A");
        tags.insert("album", "Box");
        tags.insert("tracknumber", "1");
        files.push(ScannedFile {
            path: format!("/m/A/Box/CD{disc}/01.flac"),
            size: 100,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        });
    }
    let c = build(files, vec!["/m".into()], 0);
    assert_eq!(c.releases.len(), 1);
    let discs: Vec<Option<u32>> = c.tracks.iter().map(|t| t.disc_no).collect();
    assert_eq!(discs, vec![Some(1), Some(2)], "read from the folder");
}

#[test]
fn two_editions_in_two_folders_are_still_two_albums() {
    // The folder is in the release key to tell two editions apart — a CD
    // rip beside a vinyl rip. Folding disc folders in must not fold those.
    let edition = |folder: &str| {
        let mut tags = RawTags::default();
        tags.insert("title", "T");
        tags.insert("artist", "A");
        tags.insert("albumartist", "A");
        tags.insert("album", "Album");
        tags.insert("tracknumber", "1");
        ScannedFile {
            path: format!("/m/A/{folder}/01.flac"),
            size: 100,
            mtime: 0,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        }
    };
    let c = build(
        vec![edition("Album (CD rip)"), edition("Album (vinyl rip)")],
        vec!["/m".into()],
        0,
    );
    assert_eq!(c.releases.len(), 2, "two editions stay two");
}

#[test]
fn compilation_has_no_album_artist() {
    let c = example_catalog();
    let duos = first_release(&c, "Duos").expect("compilation found");
    assert!(duos.is_compilation);
    assert_eq!(duos.album_artist_id, None);
}

#[test]
fn interning_reuses_entities_and_keeps_ids_contiguous() {
    let mut b = Builder::new(vec!["/m".into()], 0);
    let first = b.intern_artist("The Beatles");
    assert_eq!(
        first,
        b.intern_artist("Beatles, The"),
        "normalization matches them"
    );
    assert_eq!(b.intern_artist("Björk"), 1, "a new name takes the next id");
    assert_eq!(b.intern_label("Columbia"), 0);
    assert_eq!(b.intern_genre("Jazz"), 0);
    let catalog = b.finish();
    assert_eq!(catalog.artists.len(), 2);
    for (index, artist) in catalog.artists.iter().enumerate() {
        assert_eq!(artist.id as usize, index, "ids index the vector");
    }
}

#[test]
fn the_same_credit_is_never_recorded_twice() {
    let mut b = Builder::new(vec![], 0);
    let artist = b.intern_artist("Miles Davis");
    b.push_credit(artist, EntityKind::Track, 0, "main");
    b.push_credit(artist, EntityKind::Track, 0, "main");
    b.push_credit(artist, EntityKind::Track, 0, "composer");
    let catalog = b.finish();
    assert_eq!(
        catalog.credits.len(),
        2,
        "same role once, different role kept"
    );
}

#[test]
fn placeholder_album_artists_are_recognised() {
    for name in ["Various Artists", "various", "VA", "Artistes divers"] {
        assert!(is_various_artists(name), "{name} should be a placeholder");
    }
    for name in ["Various Cruelties", "Miles Davis"] {
        assert!(!is_various_artists(name), "{name} is a real artist");
    }
}

#[test]
fn title_inferred_from_filename() {
    assert_eq!(title_from_filename("/m/01 - So What.flac"), "So What");
    assert_eq!(title_from_filename("/m/So What.flac"), "So What");
    assert_eq!(track_from_filename("/m/07 - Blue in Green.flac"), Some(7));
    assert_eq!(track_from_filename("/m/Blue in Green.flac"), None);
}

#[test]
fn deterministic_build() {
    let a = example_catalog();
    let b = example_catalog();
    let names_a: Vec<&str> = a.artists.iter().map(|x| x.name.as_str()).collect();
    let names_b: Vec<&str> = b.artists.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names_a, names_b, "identifiers must be stable");
}
