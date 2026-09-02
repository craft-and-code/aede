//! Tests for [`super`], split out of `artwork.rs`.
//!
//! These run against **real containers**: a copy of each reference file, with a
//! picture written into it, read back out. A fixture of hand-assembled bytes
//! would only prove that this module agrees with whoever wrote the fixture,
//! which on a question of "does this work for MP4 as well as FLAC" is exactly
//! the thing worth doubting.

use super::*;

/// A one-pixel PNG, small enough to sit in a test and real enough to be one.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Copies a reference file into a temporary place and writes a picture into it.
///
/// `None` when this build of `lofty` will not put a picture in that container —
/// which is an answer about the format, not a failure of this module, and is
/// reported as a skip rather than as a pass.
fn with_picture(
    case: &str,
    name: &str,
    kind: lofty::picture::PictureType,
) -> Option<std::path::PathBuf> {
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile as _, TaggedFileExt};
    use lofty::picture::{MimeType, Picture};
    use lofty::tag::Accessor as _;

    // A directory per test: two tests writing one temporary name is a race
    // that fails as "this format cannot carry a picture", which is a lie about
    // the format and would have been believed.
    let dir = std::env::temp_dir().join(format!("aede_artwork_{case}"));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    let target = dir.join(name);
    let _ = std::fs::remove_file(&target);
    std::fs::copy(fixture(name), &target).expect("a copy of the fixture");

    let mut file = lofty::read_from_path(&target).ok()?;
    let kind_of_tag = file.primary_tag_type();
    if file.primary_tag().is_none() {
        file.insert_tag(lofty::tag::Tag::new(kind_of_tag));
    }
    let tag = file.primary_tag_mut()?;
    tag.set_title("A track".to_string());
    tag.push_picture(
        Picture::unchecked(PNG.to_vec())
            .pic_type(kind)
            .mime_type(MimeType::Png)
            .build(),
    );
    file.save_to_path(&target, WriteOptions::default()).ok()?;

    // Writing may report success and still not have put a picture in the file
    // — some containers are written through a tag that carries none. Asking
    // *this crate's own reader* whether the file now has artwork is what tells
    // a format extraction cannot handle from one the test failed to prepare,
    // and only the first of those is a defect here.
    match crate::tags::read(&target).map(|tags| tags.has_embedded_art) {
        Ok(true) => Some(target),
        _ => None,
    }
}

#[test]
fn a_picture_comes_back_out_of_every_container_it_went_into() {
    // The point of the whole module, and the thing the user asked for: not
    // FLAC alone. Each of these is a different way of carrying an image — a
    // FLAC metadata block, an ID3 `APIC` frame, an MP4 `covr` atom, a
    // base64-wrapped block inside an Ogg comment.
    let mut proven = 0;
    for name in [
        "track.flac",
        "track.mp3",
        "track.m4a",
        "track.ogg",
        "track.opus",
        "track.aiff",
        "track.wav",
        "track.wv",
    ] {
        let Some(path) = with_picture("all", name, lofty::picture::PictureType::CoverFront) else {
            // The test could not *prepare* a file of this format — which says
            // nothing about whether extraction works on one written by a real
            // tagger. Saying so beats a test that quietly proves nothing.
            eprintln!("skipped: no picture could be written into {name}");
            continue;
        };
        assert_eq!(
            embedded(&path).expect("readable").as_deref(),
            Some(PNG),
            "the image written into {name} is the image that came back"
        );
        proven += 1;
        let _ = std::fs::remove_file(&path);
    }
    assert!(
        proven >= 4,
        "only {proven} containers were actually exercised, which is too few to \
         claim this works beyond FLAC"
    );
}

#[test]
fn a_file_carrying_no_picture_answers_nothing() {
    // Not an error: most files have no artwork, and a failure there would make
    // the ordinary case look like a fault.
    assert_eq!(embedded(&fixture("track.flac")).expect("readable"), None);
}

#[test]
fn the_front_wins_over_the_back() {
    // A folder image that turned out to be the back of the sleeve would be
    // worse than none, because nothing afterwards would look again.
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile as _, TaggedFileExt};
    use lofty::picture::{MimeType, Picture, PictureType};

    let Some(path) = with_picture("front", "track.flac", PictureType::Media) else {
        panic!("FLAC must accept a picture");
    };
    // A second image, typed as the front, added after the first.
    let mut file = lofty::read_from_path(&path).expect("readable");
    let front = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00];
    file.primary_tag_mut().expect("a tag").push_picture(
        Picture::unchecked(front.clone())
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build(),
    );
    file.save_to_path(&path, WriteOptions::default())
        .expect("saved");

    assert_eq!(
        embedded(&path).expect("readable"),
        Some(front),
        "the front is chosen however late it appears in the file"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_file_that_is_not_audio_is_an_error_and_not_an_empty_answer() {
    let path = std::env::temp_dir().join("aede_artwork_nonsense.flac");
    std::fs::write(&path, b"not a FLAC file at all").expect("written");
    assert!(embedded(&path).is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_picture_written_beside_the_music_is_the_one_the_file_carried() {
    // End to end, on a real container: the image goes into a FLAC, and the
    // file that lands on disk is byte for byte what came out of it.
    let Some(source) = with_picture(
        "into",
        "track.flac",
        lofty::picture::PictureType::CoverFront,
    ) else {
        panic!("FLAC must accept a picture");
    };
    let folder = source.parent().expect("a folder");

    let written = extract_into(&source, folder).expect("a file was written");
    // A PNG stays a PNG: the name follows what the bytes are, not what the
    // caller would have preferred them to be.
    assert_eq!(
        written.file_name().and_then(|n| n.to_str()),
        Some("cover.png")
    );
    assert_eq!(std::fs::read(&written).expect("readable"), PNG);

    // Running again changes nothing rather than writing over it — the guard
    // that matters, and the reason it lives here and not in each caller.
    assert!(
        extract_into(&source, folder).is_err(),
        "what is already there is what stays there"
    );
    let _ = std::fs::remove_file(&written);
}

#[test]
fn every_picture_a_file_carries_comes_back_with_what_it_is_of() {
    // A real container with three pictures of three kinds, read back out —
    // which is what `--images` rests on, and what a fixture of hand-assembled
    // bytes would prove nothing about.
    use lofty::config::WriteOptions;
    use lofty::file::{AudioFile as _, TaggedFileExt};
    use lofty::picture::{MimeType, Picture, PictureType};

    let Some(path) = with_picture("kinds", "track.flac", PictureType::CoverFront) else {
        panic!("FLAC must accept a picture");
    };
    let mut file = lofty::read_from_path(&path).expect("readable");
    let back = vec![0xFF, 0xD8, 0xFF, 0xE0, b'b'];
    let leaflet = vec![0xFF, 0xD8, 0xFF, 0xE0, b'l'];
    for (bytes, kind) in [
        (back.clone(), PictureType::CoverBack),
        (leaflet.clone(), PictureType::Leaflet),
    ] {
        file.primary_tag_mut().expect("a tag").push_picture(
            Picture::unchecked(bytes)
                .pic_type(kind)
                .mime_type(MimeType::Jpeg)
                .build(),
        );
    }
    file.save_to_path(&path, WriteOptions::default())
        .expect("saved");

    let all = pictures(&path).expect("readable");
    assert_eq!(
        all.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        vec![Kind::Front, Kind::Back, Kind::Booklet],
        "the tag's own classification, narrowed to the kinds this program uses"
    );
    assert_eq!(all[1].1, back);
    assert_eq!(all[2].1, leaflet);

    // And the front-only reader still answers the front, out of the three.
    assert_eq!(embedded(&path).expect("readable").as_deref(), Some(PNG));

    // Written out: the cover stays beside the music, everything else goes one
    // level down — a back sleeve next to the tracks would be taken for the
    // album's cover by this program's own scanner.
    let folder = path.parent().expect("a folder");
    let out = extract_extras_into(&path, folder).expect("readable");
    assert_eq!(out.len(), 2, "the front is not one of the extras");
    let into = crate::coverart::extras_in(folder);
    assert_eq!(
        out.iter()
            .map(|one| match &one.wrote {
                Ok(crate::coverart::Written::New(p)) => p.clone(),
                other => panic!("expected a written file, got {other:?}"),
            })
            .collect::<Vec<_>>(),
        vec![into.join("back.jpg"), into.join("booklet.jpg")],
        "one of each kind, so neither is numbered"
    );
    assert_eq!(
        std::fs::read(into.join("back.jpg")).expect("readable"),
        back
    );
    assert!(!folder.join("back.jpg").exists());

    // Twice over changes nothing, and says so without calling it a failure.
    let again = extract_extras_into(&path, folder).expect("readable");
    assert!(
        again
            .iter()
            .all(|one| matches!(one.wrote, Ok(crate::coverart::Written::Already(_)))),
        "what is already there is what stays there"
    );
    let _ = std::fs::remove_dir_all(&into);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_file_carrying_nothing_but_a_cover_has_no_other_images() {
    let Some(path) = with_picture(
        "only_front",
        "track.flac",
        lofty::picture::PictureType::CoverFront,
    ) else {
        panic!("FLAC must accept a picture");
    };
    let folder = path.parent().expect("a folder");
    assert!(
        extract_extras_into(&path, folder)
            .expect("readable")
            .is_empty()
    );
    // Nothing to write means no folder made: an empty `artwork/` in every
    // album would be litter, and would also read as "this one is done".
    assert!(!crate::coverart::extras_in(folder).exists());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_file_with_no_picture_is_a_refusal_and_not_an_empty_file() {
    let dir = std::env::temp_dir().join("aede_artwork_bare");
    std::fs::create_dir_all(&dir).expect("a folder");
    assert!(extract_into(&fixture("track.flac"), &dir).is_err());
    assert!(!dir.join("cover.jpg").exists());
    assert!(!dir.join("cover.png").exists());
}
