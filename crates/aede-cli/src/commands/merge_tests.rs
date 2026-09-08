//! Tests for [`super`], split out of `merge.rs`.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::RawTags;

/// A catalog holding one artist per name given, with an optional identifier.
fn shelf(artists: &[(&str, Option<&str>)]) -> Catalog {
    let files = artists
        .iter()
        .enumerate()
        .map(|(at, (name, mbid))| {
            let mut tags = RawTags::default();
            tags.insert("artist", *name);
            tags.insert("albumartist", *name);
            tags.insert("album", format!("Album {at}"));
            tags.insert("title", format!("Track {at}"));
            if let Some(id) = mbid {
                tags.insert("musicbrainz_artistid", *id);
            }
            ScannedFile {
                path: format!("/music/{at}.flac"),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    build(files, vec!["/music".to_string()], 1, &[])
}

/// Where a run of these tests keeps its `user.json`.
fn scratch(what: &str) -> std::path::PathBuf {
    // Thread name **and** argument: unique across tests however they arrive
    // here, and unique within a test that wants two.
    let named = std::thread::current().name().unwrap_or("merge").to_string();
    let dir = std::env::temp_dir().join(format!("aede-merge-{}-{what}", named.replace("::", "-")));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("user.json")
}

#[test]
fn a_statement_is_kept_normalised_and_read_back() {
    // Normalised on the way in, because that is the form every merge in this
    // program is decided on: a statement about `Ozzy  Osbourne` has to be
    // about `ozzy osbourne` on the next scan, whatever was typed.
    let path = scratch("kept");
    let catalog = shelf(&[("Ozzy Osbourne", None), ("O. Osbourne", None)]);
    let mut data = UserData::default();
    state(&catalog, &mut data, &path, "O. Osbourne", "Ozzy  Osbourne").unwrap();

    let read = user::load(&path).unwrap().expect("a file was written");
    assert_eq!(read.same_artist.len(), 1);
    assert_eq!(read.same_artist[0].spelling, "o osbourne");
    assert_eq!(read.same_artist[0].filed_as, "ozzy osbourne");
    assert_eq!(
        chosen(&read),
        vec![aede_core::model::identity::Chosen {
            spelling: "o osbourne".into(),
            filed_as: "ozzy osbourne".into(),
        }],
        "and it reaches the scan in the shape the builder wants"
    );
}

#[test]
fn two_identifiers_are_two_people_and_the_merge_is_refused() {
    // The one thing this command knows better than the person typing. A shared
    // identifier already merges two spellings with no heuristic; two different
    // ones are two musicians, said so by the only authority on the question,
    // and the place to argue is MusicBrainz rather than a file on this disk.
    let path = scratch("refused");
    let catalog = shelf(&[("Angus Young", Some("angus")), ("Neil Young", Some("neil"))]);
    let mut data = UserData::default();
    let refusal = state(&catalog, &mut data, &path, "Angus Young", "Neil Young")
        .expect_err("two identifiers are two people");
    let said = refusal.to_string();
    assert!(said.contains("angus") && said.contains("neil"), "{said}");
    assert!(said.contains("at the source"), "{said}");
    assert!(data.same_artist.is_empty(), "and nothing was written");
    assert!(!path.exists(), "not even a file");
}

#[test]
fn one_identifier_and_one_silence_is_not_a_contradiction() {
    // The ordinary case: a well-tagged album and an old rip that carries
    // nothing. Refusing here would leave the very libraries this exists for
    // unable to say anything.
    let path = scratch("half");
    let catalog = shelf(&[("Ozzy Osbourne", Some("ozzy")), ("O. Osbourne", None)]);
    let mut data = UserData::default();
    state(&catalog, &mut data, &path, "O. Osbourne", "Ozzy Osbourne").unwrap();
    assert_eq!(data.same_artist.len(), 1);
}

#[test]
fn a_spelling_the_shelf_has_never_held_is_still_a_statement() {
    // A merge stated before the folder holding it is scanned is not a mistake,
    // and refusing it would make the order of two commands matter for nothing.
    // The command says which half it did not recognise; it does not refuse.
    let path = scratch("unknown");
    let catalog = shelf(&[("Ozzy Osbourne", None)]);
    let mut data = UserData::default();
    state(&catalog, &mut data, &path, "Ozzy O.", "Ozzy Osbourne").unwrap();
    assert_eq!(data.same_artist.len(), 1);
}

#[test]
fn saying_it_again_with_another_destination_is_a_correction() {
    // One spelling gives way to one artist. Two rows would leave the shelf to
    // arbitrate between them, which is what this program never does.
    let path = scratch("correction");
    let catalog = shelf(&[("Ozzy Osbourne", None), ("Ozzy O.", None)]);
    let mut data = UserData::default();
    state(&catalog, &mut data, &path, "O. Osbourne", "Ozzy O.").unwrap();
    state(&catalog, &mut data, &path, "O. Osbourne", "Ozzy Osbourne").unwrap();
    assert_eq!(data.same_artist.len(), 1, "{:?}", data.same_artist);
    assert_eq!(data.same_artist[0].filed_as, "ozzy osbourne");
}

#[test]
fn a_name_that_is_already_one_name_here_is_refused_rather_than_stored() {
    // `Ozzy  Osbourne` and `ozzy osbourne` are one key everywhere in this
    // program. Storing a row filing a name under itself would make every
    // lookup that misses look like one that hit.
    let path = scratch("same");
    let catalog = shelf(&[("Ozzy Osbourne", None)]);
    let mut data = UserData::default();
    let refusal = state(
        &catalog,
        &mut data,
        &path,
        "Ozzy  Osbourne",
        "ozzy osbourne",
    )
    .expect_err("nothing to merge");
    assert!(
        refusal.to_string().contains("already one name"),
        "{refusal}"
    );
}
