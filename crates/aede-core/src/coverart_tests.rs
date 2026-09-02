//! Tests for [`super`], split out of `coverart.rs`.
//!
//! The fixture below is the shape the real service answers with — its keys
//! were read off a live response rather than assumed, which is not true of
//! every module in this crate and is worth saying where it is.

use super::*;
use crate::json::parse;

/// An index as `coverartarchive.org/release-group/<mbid>` answers.
///
/// Abbreviated: the real document repeats the same keys per image, and a
/// fixture reproducing all of them would hide what is being asserted.
const INDEX: &str = r#"{
  "images": [
    { "id": 111, "front": false, "back": true, "approved": true,
      "types": ["Back"], "image": "https://coverartarchive.org/x/111.jpg",
      "thumbnails": { "250": "…/111-250.jpg", "500": "…/111-500.jpg",
                      "1200": "…/111-1200.jpg" } },
    { "id": 222, "front": true, "back": false, "approved": true,
      "types": ["Front"], "image": "https://coverartarchive.org/x/222.jpg",
      "thumbnails": { "250": "https://coverartarchive.org/x/222-250.jpg",
                      "500": "https://coverartarchive.org/x/222-500.jpg",
                      "1200": "https://coverartarchive.org/x/222-1200.jpg",
                      "small": "…", "large": "…" } }
  ],
  "release": "https://musicbrainz.org/release/956fbc58"
}"#;

fn json(text: &str) -> crate::json::Json {
    parse(text).expect("the fixture is valid JSON")
}

#[test]
fn the_front_image_is_chosen_and_the_back_is_not() {
    let found = front(&json(INDEX), Size::Thumbnail(1200)).expect("a front image");
    assert_eq!(found.url, "https://coverartarchive.org/x/222-1200.jpg");
    assert_eq!(found.size, Size::Thumbnail(1200));

    let original = front(&json(INDEX), Size::Original).expect("a front image");
    assert_eq!(original.url, "https://coverartarchive.org/x/222.jpg");
    assert_eq!(original.size, Size::Original);
}

#[test]
fn an_image_marked_front_only_in_its_types_is_still_the_front() {
    // Both fields are filled by editors, and an image set in one and not the
    // other is common enough that reading only `front` leaves records looking
    // as though they have no cover at all.
    let doc = json(
        r#"{"images":[{"id":1,"types":["Front"],
             "image":"https://coverartarchive.org/x/1.jpg","thumbnails":{}}]}"#,
    );
    assert_eq!(
        front(&doc, Size::Thumbnail(500)).map(|f| f.url),
        Some("https://coverartarchive.org/x/1.jpg".to_string()),
        "and with no thumbnail of that width, the original stands in"
    );
}

#[test]
fn a_missing_thumbnail_falls_back_rather_than_answering_nothing() {
    // A width absent from the answer must not look like a record with no
    // artwork: those are two different things, and the second is the one this
    // whole pass exists to find.
    let doc = json(
        r#"{"images":[{"front":true,"image":"https://coverartarchive.org/x/9.jpg",
             "thumbnails":{"250":"https://coverartarchive.org/x/9-250.jpg"}}]}"#,
    );
    let found = front(&doc, Size::Thumbnail(1200)).expect("a front image");
    assert_eq!(found.url, "https://coverartarchive.org/x/9.jpg");
    assert_eq!(
        found.size,
        Size::Original,
        "and it says what it actually got, rather than what was asked"
    );
}

#[test]
fn an_approved_image_wins_over_one_nobody_has_looked_at() {
    // Anyone may upload; `approved` is an editor having looked.
    let doc = json(
        r#"{"images":[
             {"front":true,"approved":false,"image":"https://x/unchecked.jpg","thumbnails":{}},
             {"front":true,"approved":true,"image":"https://x/checked.jpg","thumbnails":{}}]}"#,
    );
    assert_eq!(
        front(&doc, Size::Original).map(|f| f.url),
        Some("https://x/checked.jpg".to_string())
    );

    // But an unapproved cover is still a cover, and better than none.
    let only_unapproved = json(
        r#"{"images":[{"front":true,"approved":false,
             "image":"https://x/unchecked.jpg","thumbnails":{}}]}"#,
    );
    assert!(front(&only_unapproved, Size::Original).is_some());
}

#[test]
fn a_record_with_no_front_image_answers_nothing() {
    for body in [
        r#"{"images":[]}"#,
        r#"{"images":[{"back":true,"types":["Back"],"image":"https://x/b.jpg"}]}"#,
        r#"{"release":"https://musicbrainz.org/release/x"}"#,
        // A front image with no address is not an address.
        r#"{"images":[{"front":true,"thumbnails":{}}]}"#,
    ] {
        assert_eq!(front(&json(body), Size::Original), None, "body: {body}");
    }
}

#[test]
fn only_bytes_that_are_actually_an_image_are_written() {
    // The guard that matters most in this whole pass: a download that went
    // wrong — a redirect to an error page, a truncated transfer — arrives as
    // bytes like any other, and writing them into a music folder as `cover.jpg`
    // is silent corruption nobody notices for months.
    assert_eq!(image_kind(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]), Some("jpg"));
    assert_eq!(
        image_kind(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
        Some("png")
    );
    for refused in [
        &b"<!DOCTYPE html><html>Not Found"[..],
        &b"{\"error\":\"no such release\"}"[..],
        &b""[..],
        &b"\xFF\xD8"[..],
        &b"GIF89a"[..],
    ] {
        assert_eq!(image_kind(refused), None, "refused: {refused:?}");
    }
}

#[test]
fn a_size_is_one_the_archive_actually_generates() {
    assert_eq!(Size::parse("1200"), Some(Size::Thumbnail(1200)));
    assert_eq!(Size::parse("500"), Some(Size::Thumbnail(500)));
    assert_eq!(Size::parse("250"), Some(Size::Thumbnail(250)));
    assert_eq!(Size::parse(" Original "), Some(Size::Original));
    // A width nobody generates would be asked for, missed, and quietly fall
    // back — so it is refused where the message can say what there is.
    for unusable in ["800", "1000", "big", "", "0"] {
        assert_eq!(Size::parse(unusable), None, "unusable: {unusable}");
    }
}

#[test]
fn the_file_is_named_what_the_scanner_already_looks_for() {
    // Nothing registers the written file anywhere: the next scan discovers it
    // exactly as it would one the user had put there by hand.
    assert_eq!(file_name("jpg"), "cover.jpg");
    assert_eq!(file_name("png"), "cover.png");
}

#[test]
fn a_kind_survives_the_round_trip_through_the_word_a_source_uses() {
    for (word, kind) in [
        ("Front", Kind::Front),
        ("back", Kind::Back),
        ("Booklet", Kind::Booklet),
        ("Medium", Kind::Media),
        ("media", Kind::Media),
        ("Artist", Kind::Artist),
    ] {
        assert_eq!(Kind::parse(word), kind, "word: {word}");
    }
    // An image nobody classified is still an image: losing it because the
    // label was unfamiliar would be a strange way to fail.
    for unknown in ["Sticker", "", "Raw", "Poster"] {
        assert_eq!(Kind::parse(unknown), Kind::Other, "unknown: {unknown}");
    }
    // The front is spelt `cover`, because that is the name every player and
    // this program's own scanner look for first.
    assert_eq!(Kind::Front.stem(), "cover");
    assert_eq!(file_name("jpg"), format!("{}.jpg", Kind::Front.stem()));
}

#[test]
fn a_lone_image_is_not_numbered_and_a_set_of_them_is() {
    // `booklet-01.jpg` on its own reads as a page torn out of something.
    assert_eq!(image_name(Kind::Back, "jpg", 0, 1), "back.jpg");
    assert_eq!(image_name(Kind::Back, "png", 0, 0), "back.png");
    assert_eq!(image_name(Kind::Booklet, "jpg", 0, 3), "booklet-01.jpg");
    assert_eq!(image_name(Kind::Booklet, "jpg", 2, 3), "booklet-03.jpg");
    // Two digits, so that ten pages sort after nine rather than between one
    // and two in every file manager there is.
    assert_eq!(image_name(Kind::Booklet, "jpg", 9, 12), "booklet-10.jpg");
}

#[test]
fn each_image_is_numbered_within_its_own_kind() {
    // The numbering a downloaded booklet and an extracted one must share: it
    // is worked out in one place precisely so they cannot drift apart.
    let kinds = [
        Kind::Back,
        Kind::Booklet,
        Kind::Booklet,
        Kind::Media,
        Kind::Booklet,
    ];
    assert_eq!(
        positions(&kinds),
        vec![(0, 1), (0, 3), (1, 3), (0, 1), (2, 3)]
    );
    assert!(positions(&[]).is_empty());

    // Which gives: one back, three numbered booklet pages, one disc label.
    let names: Vec<String> = kinds
        .iter()
        .zip(positions(&kinds))
        .map(|(kind, (index, of))| image_name(*kind, "jpg", index, of))
        .collect();
    assert_eq!(
        names,
        [
            "back.jpg",
            "booklet-01.jpg",
            "booklet-02.jpg",
            "media.jpg",
            "booklet-03.jpg"
        ]
    );
}

#[test]
fn every_image_of_an_index_comes_back_with_what_it_is_of() {
    let all = images(&json(INDEX), Size::Thumbnail(1200));
    // The front first, whatever order the archive listed them in: it is the
    // one image the rest of the program cares about.
    assert_eq!(
        all.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(),
        vec![Kind::Front, Kind::Back]
    );
    assert_eq!(all[0].1.url, "https://coverartarchive.org/x/222-1200.jpg");
    assert_eq!(all[1].1.url, "…/111-1200.jpg");

    // An image with no address at all is not an address, and is left out
    // rather than carried along as an empty one.
    let doc = json(
        r#"{"images":[{"types":["Booklet"],"thumbnails":{}},
             {"types":["Booklet"],"image":"https://x/2.jpg","thumbnails":{}}]}"#,
    );
    let some = images(&doc, Size::Original);
    assert_eq!(some.len(), 1);
    assert_eq!(some[0].0, Kind::Booklet);

    assert!(images(&json(r#"{"release":"https://x"}"#), Size::Original).is_empty());
}

#[test]
fn what_is_not_the_cover_goes_one_level_down() {
    // The whole reason the subfolder exists: `scan::cover_in` takes any image
    // in a folder for a candidate cover, so a `back.jpg` beside the music
    // would become the album's artwork — the wrong picture, and one nothing
    // would ever look at again.
    let folder = std::path::Path::new("/music/Nirvana/Nevermind");
    assert_eq!(extras_in(folder), folder.join(EXTRAS));
    assert_ne!(EXTRAS, "");
}

#[test]
fn the_other_images_are_written_into_a_folder_that_did_not_exist() {
    let dir = std::env::temp_dir().join("aede_coverart_extras");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let into = extras_in(&dir);
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];

    // The subfolder is made on the way, because it will not be there the
    // first time and the caller should not have to know that.
    let first = write_image(&into, Kind::Booklet, (0, 2), &jpeg).expect("written");
    assert_eq!(first, Written::New(into.join("booklet-01.jpg")));
    assert!(into.is_dir());

    // A second run says the file is already there — which is not a failure,
    // and must not be counted as one: running twice over a library would
    // otherwise report every folder as an error for having worked.
    let again = write_image(&into, Kind::Booklet, (0, 2), &jpeg).expect("not an error");
    assert_eq!(again, Written::Already(into.join("booklet-01.jpg")));
    assert_eq!(
        std::fs::read(into.join("booklet-01.jpg")).expect("read"),
        jpeg
    );

    // And the sniff guard is the same one, in the same place.
    assert!(write_image(&into, Kind::Back, (0, 1), b"<!DOCTYPE html>").is_err());
    assert!(!into.join("back.jpg").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nothing_but_an_image_reaches_a_music_folder() {
    let dir = std::env::temp_dir().join("aede_coverart_write");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");

    // Not an image: refused, and the folder is left as it was.
    assert!(write_beside(&dir, b"<!DOCTYPE html>Not Found").is_err());
    assert!(!dir.join("cover.jpg").exists());
    assert!(!dir.join("cover.png").exists());

    // An image: written, under the name that says what it is.
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    let path = write_beside(&dir, &jpeg).expect("written");
    assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("cover.jpg"));
    assert_eq!(std::fs::read(&path).expect("readable"), jpeg);

    // And never twice. There is no flag anywhere that makes this overwrite.
    assert!(write_beside(&dir, &jpeg).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_folder_never_ends_up_with_two_covers() {
    // The hole this closes: extraction writes `cover.png`, a download writes
    // `cover.jpg`, neither sees the other because each only checks the name it
    // is about to write — and both rank as the album's cover, so which one a
    // player shows is anybody's guess. It was the 1200 px download over the
    // full-size picture that had been inside the files all along.
    let dir = std::env::temp_dir().join("aede_coverart_two");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");

    let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];
    let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    write_beside(&dir, &png).expect("the first one is written");

    let refused = write_beside(&dir, &jpeg).expect_err("and the second is not");
    assert!(refused.contains("already the cover"), "{refused}");
    assert!(!dir.join("cover.jpg").exists());

    // Any name the scanner would take as a cover counts, not only `cover.*`:
    // the question asked is the scanner's own.
    let other = std::env::temp_dir().join("aede_coverart_two_folder");
    let _ = std::fs::remove_dir_all(&other);
    std::fs::create_dir_all(&other).expect("a folder");
    std::fs::write(other.join("folder.jpg"), jpeg).expect("written");
    assert!(write_beside(&other, &png).is_err());
    assert!(!other.join("cover.png").exists());

    // The images that are not the cover are untouched by this: they live in
    // their own folder, where the first one written must not block the rest.
    let into = extras_in(&dir);
    assert!(matches!(
        write_image(&into, Kind::Back, (0, 1), &jpeg),
        Ok(Written::New(_))
    ));
    assert!(matches!(
        write_image(&into, Kind::Booklet, (0, 1), &jpeg),
        Ok(Written::New(_))
    ));

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other);
}
