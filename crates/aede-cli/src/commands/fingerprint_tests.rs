//! Tests for [`super`], split out of `fingerprint.rs`.
//!
//! About which files the command picks, which is where its judgement is. The
//! decoding itself is proved in `aede_core::fingerprint`, against a real
//! ffmpeg.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::{AudioProperties, RawTags};

/// A library of one file per entry: `(title, artist, seconds)`.
fn library(files: &[(&str, &str, u64)]) -> Catalog {
    tagged(files, false)
}

/// The same, with a MusicBrainz recording identifier in every file's tags —
/// which is what a file that has been through Picard looks like.
fn from_picard(files: &[(&str, &str, u64)]) -> Catalog {
    tagged(files, true)
}

fn tagged(files: &[(&str, &str, u64)], recording_id: bool) -> Catalog {
    let scanned = files
        .iter()
        .enumerate()
        .map(|(n, (title, artist, ms))| {
            let mut tags = RawTags::default();
            if !title.is_empty() {
                tags.insert("title", *title);
            }
            if !artist.is_empty() {
                tags.insert("artist", *artist);
                tags.insert("albumartist", *artist);
            }
            tags.insert("album", "An album");
            if recording_id {
                tags.insert("musicbrainz_trackid", "a3e4f5c6");
            }
            tags.properties = AudioProperties {
                duration_ms: (*ms > 0).then_some(*ms),
                ..Default::default()
            };
            ScannedFile {
                path: format!("/music/folder{n}/0{n}.flac"),
                size: n as u64 + 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
                fingerprint: None,
            }
        })
        .collect();
    build(scanned, vec!["/music".to_string()], 1)
}

#[test]
fn only_the_files_the_tags_cannot_identify() {
    // The whole point: decoding a library that is already correctly tagged is
    // hours of work to confirm what the tags say.
    let catalog = library(&[
        ("So What", "Miles Davis", 545_000),
        ("", "", 183_000),
        ("", "Miles Davis", 200_000),
        ("Blue in Green", "", 327_000),
    ]);
    let found = survey(&catalog, &[], &[], false);
    assert_eq!(
        found.targets.len(),
        3,
        "a file missing either half of its name needs rescuing"
    );
    assert_eq!(found.named, 1);

    // `--full` lifts that, because "do it all" means all.
    assert_eq!(survey(&catalog, &[], &[], true).targets.len(), 4);
}

#[test]
fn a_file_with_no_length_is_counted_rather_than_decoded() {
    // A lookup is a fingerprint *and* a length. Decoding a file whose length
    // nothing knows would produce something that cannot be asked about — work
    // done for an answer that could never be sent.
    let catalog = library(&[("", "", 0)]);
    let found = survey(&catalog, &[], &[], false);
    assert!(found.targets.is_empty());
    assert_eq!(found.no_length, 1);
    // And `--full` does not lift this one: it is not a preference, it is a
    // missing input.
    assert!(survey(&catalog, &[], &[], true).targets.is_empty());
}

#[test]
fn a_file_already_fingerprinted_is_not_decoded_twice() {
    // Decoding is the expensive half, and the stored answer is exact: the
    // same bytes give the same fingerprint, so recomputing it buys nothing.
    let mut catalog = library(&[("", "", 183_000)]);
    catalog.files[0].fingerprint = Some(aede_core::fingerprint::Fingerprint {
        data: "AQAAcxUmUaEk".to_string(),
        seconds: 183,
    });
    let found = survey(&catalog, &[], &[], false);
    assert!(found.targets.is_empty());
    assert_eq!(found.done, 1);

    assert_eq!(
        survey(&catalog, &[], &[], true).targets.len(),
        1,
        "--full computes it again"
    );
}

#[test]
fn the_length_offered_is_whole_seconds_of_what_the_header_said() {
    let catalog = library(&[("", "", 183_400)]);
    let found = survey(&catalog, &[], &[], false);
    assert_eq!(found.targets[0].seconds, 183, "truncated, never rounded up");
}

#[test]
fn a_folder_narrows_it_and_a_name_reaches_a_path() {
    // A file this command is about often has no tags at all, so a name mostly
    // means "this folder" — and the path is one of the things a name is
    // matched against for exactly that reason.
    let catalog = library(&[("", "", 183_000), ("", "", 200_000)]);
    let scoped = survey(&catalog, &["/music/folder1".to_string()], &[], false);
    assert_eq!(scoped.targets.len(), 1);
    assert!(scoped.targets[0].path.contains("folder1"));

    let named = survey(&catalog, &[], &["folder0".to_string()], false);
    assert_eq!(named.targets.len(), 1);
    assert!(named.targets[0].path.contains("folder0"));
}

#[test]
fn a_file_that_already_carries_a_recording_id_is_not_decoded_for_one() {
    // The sharpest of the four skips. A file tagged by Picard holds
    // `musicbrainz_recordingid`, which is exactly what an AcoustID lookup
    // would answer with — decoding it spends minutes and a request to be told
    // something the file already says.
    //
    // Note the tags here name nothing: title and artist are empty, so every
    // other rule would have picked this file up. The identifier is what saves
    // the work, on its own.
    let catalog = from_picard(&[("", "", 183_000)]);
    let found = survey(&catalog, &[], &[], false);
    assert!(found.targets.is_empty());
    assert_eq!(found.already_identified, 1);
    assert_eq!(found.named, 0, "it is not the title that saved it");

    // `--full` lifts it, because a *wrong* recording identifier is the one
    // thing nothing else in this program can catch.
    assert_eq!(survey(&catalog, &[], &[], true).targets.len(), 1);
}
