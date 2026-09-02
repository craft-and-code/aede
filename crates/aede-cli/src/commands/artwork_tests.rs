//! What `artwork` writes, and what it refuses to touch.
//!
//! Declared in `artwork.rs` with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::tags::RawTags;

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_cli_artwork_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A library of one file per folder named, each folder really on disk.
fn library(dir: &std::path::Path, folders: &[(&str, bool)]) -> Catalog {
    let files = folders
        .iter()
        .map(|(name, embedded)| {
            let folder = dir.join("music").join(name);
            std::fs::create_dir_all(&folder).expect("a folder");
            let mut tags = RawTags::default();
            tags.insert("artist", "Miles Davis");
            tags.insert("albumartist", "Miles Davis");
            tags.insert("album", *name);
            tags.insert("title", "A track");
            tags.has_embedded_art = *embedded;
            ScannedFile {
                path: folder.join("01.flac").to_string_lossy().to_string(),
                size: 1,
                mtime: 1,
                tags,
                folder_cover: None,
                sidecar: None,
                integrity: None,
            }
        })
        .collect();
    build(
        files,
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
    )
}

#[test]
fn a_folder_is_the_unit_and_not_an_album() {
    // A double album is one release and two folders, and a cover image belongs
    // to a folder — that is where every player looks for it. Walking releases
    // would leave the second disc without one.
    let dir = sandbox("folders");
    let catalog = library(&dir, &[("CD1", true), ("CD2", true)]);
    let survey = survey(&catalog, &[], false);
    assert_eq!(
        survey.targets.len(),
        2,
        "both folders, though a listener would call it one album"
    );
}

#[test]
fn a_folder_that_already_holds_an_image_is_left_alone() {
    let dir = sandbox("has_image");
    let catalog = library(&dir, &[("Kind of Blue", true)]);
    // Any of the names the scanner recognises, not only `cover.jpg`: the one
    // list of names lives in `scan`, and this asks it rather than guessing.
    std::fs::write(dir.join("music/Kind of Blue/folder.png"), b"an image").expect("written");

    let survey = survey(&catalog, &[], false);
    assert!(survey.targets.is_empty());
    assert_eq!(survey.has_image, 1);
}

#[test]
fn a_folder_whose_files_carry_nothing_is_counted_rather_than_attempted() {
    // And counted separately: "there is nothing to extract" and "there is
    // already an image" are two different answers, and the first is the one
    // whose next step is `fetch --covers`.
    let dir = sandbox("nothing_inside");
    let catalog = library(&dir, &[("Kind of Blue", false)]);
    let survey = survey(&catalog, &[], false);
    assert!(survey.targets.is_empty());
    assert_eq!((survey.has_image, survey.nothing_inside), (0, 1));
}

#[test]
fn only_the_folders_named_are_looked_at() {
    let dir = sandbox("scope");
    let catalog = library(&dir, &[("CD1", true), ("CD2", true)]);
    let scope = vec![dir.join("music/CD1").to_string_lossy().to_string()];
    assert_eq!(survey(&catalog, &scope, false).targets.len(), 1);
}

#[test]
fn a_source_that_is_not_audio_is_reported_and_writes_nothing() {
    // The guards themselves live in the core, shared with the downloaded
    // covers and exercised there against real containers. What this pins is
    // that the command surfaces a refusal rather than swallowing it.
    let dir = sandbox("not_audio");
    let folder = dir.join("music/Kind of Blue");
    std::fs::create_dir_all(&folder).expect("a folder");
    let source = folder.join("01.txt");
    std::fs::write(&source, b"not audio at all").expect("written");

    let target = Target {
        folder: folder.to_string_lossy().to_string(),
        source: source.to_string_lossy().to_string(),
        cover: true,
    };
    assert!(write_one(&target).is_err());
    assert!(!folder.join("cover.jpg").exists());
    assert!(!folder.join("cover.png").exists());
    // The same refusal, and still nothing written, for the other images.
    assert!(write_others(&target).is_err());
    assert!(!coverart::extras_in(&folder).exists());
}

#[test]
fn with_images_a_folder_that_has_a_cover_is_opened_again() {
    // The cover is one question and the booklet is another. A folder finished
    // for the first can be untouched for the second, and the flag is what
    // separates them.
    let dir = sandbox("images");
    let catalog = library(&dir, &[("Kind of Blue", true)]);
    std::fs::write(dir.join("music/Kind of Blue/folder.png"), b"an image").expect("written");

    let plain = survey(&catalog, &[], false);
    assert!(plain.targets.is_empty());
    assert_eq!(plain.has_image, 1);

    let wider = survey(&catalog, &[], true);
    assert_eq!(wider.targets.len(), 1);
    assert!(
        !wider.targets[0].cover,
        "the cover is there already, and nothing overwrites it"
    );
    assert_eq!(
        wider.has_image, 0,
        "it was not skipped, so it must not be reported as skipped"
    );
}

#[test]
fn with_images_a_folder_carrying_nothing_is_still_nothing_to_do() {
    // `--images` widens which folders are opened, not which ones have
    // something inside them to write out.
    let dir = sandbox("images_empty");
    let catalog = library(&dir, &[("Kind of Blue", false)]);
    let wider = survey(&catalog, &[], true);
    assert!(wider.targets.is_empty());
    assert_eq!((wider.has_image, wider.nothing_inside), (0, 1));
}
