//! What the cover pass asks about, and what it refuses to write.
//!
//! Declared in `covers.rs` with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.

use super::*;
use aede_core::json::Json;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{Confidence, Sources};
use aede_core::tags::RawTags;

/// A transport answering JSON from canned text and bytes from canned buffers.
struct Canned {
    answers: Vec<Result<String, Refusal>>,
    bytes: Vec<Result<Vec<u8>, Refusal>>,
    asked: Vec<String>,
}

impl Canned {
    fn new(answers: Vec<Result<String, Refusal>>, bytes: Vec<Result<Vec<u8>, Refusal>>) -> Canned {
        Canned {
            answers,
            bytes,
            asked: Vec::new(),
        }
    }
}

impl Ask for Canned {
    fn get_json(&mut self, url: &str) -> Result<Json, Refusal> {
        self.asked.push(url.to_string());
        if self.answers.is_empty() {
            return Err(Refusal::Failed("nothing canned for this".to_string()));
        }
        match self.answers.remove(0) {
            Ok(text) => Ok(aede_core::json::parse(&text).expect("valid fixture")),
            Err(why) => Err(why),
        }
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
        self.asked.push(url.to_string());
        if self.bytes.is_empty() {
            return Err(Refusal::Failed("nothing canned for this".to_string()));
        }
        self.bytes.remove(0)
    }
}

/// The smallest thing `image_kind` will accept as a JPEG.
const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];

const INDEX: &str = r#"{"images":[{"front":true,"approved":true,
    "image":"https://coverartarchive.org/x/1.jpg",
    "thumbnails":{"500":"https://coverartarchive.org/x/1-500.jpg",
                  "1200":"https://coverartarchive.org/x/1-1200.jpg"}}]}"#;

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_covers_{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("music/Miles Davis/Kind of Blue")).expect("a folder");
    dir
}

/// A one-album library rooted in `dir`, so the folder really exists on disk.
fn library(dir: &std::path::Path, embedded: bool, beside: Option<&str>) -> Catalog {
    let folder = dir.join("music/Miles Davis/Kind of Blue");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("musicbrainz_albumid", "59211ea4");
    tags.has_embedded_art = embedded;
    build(
        vec![ScannedFile {
            path: folder.join("01.flac").to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: beside.map(|name| folder.join(name).to_string_lossy().to_string()),
            sidecar: None,
            integrity: None,
        }],
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
    )
}

fn cover_in(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("music/Miles Davis/Kind of Blue/cover.jpg")
}

#[test]
fn an_album_that_already_has_a_cover_is_never_asked_about() {
    // The promise of the whole command, and the catalog answers it offline:
    // an image inside the files, or one beside them, and nothing happens.
    let dir = sandbox("has_one");
    assert_eq!(
        waiting(&library(&dir, false, None), &Sources::default()),
        1,
        "no cover anywhere, so there is something to look for"
    );
    assert_eq!(
        waiting(&library(&dir, true, None), &Sources::default()),
        0,
        "the image is inside the files"
    );
    assert_eq!(
        waiting(
            &library(&dir, false, Some("cover.jpg")),
            &Sources::default()
        ),
        0,
        "and here it sits beside them"
    );
}

#[test]
fn what_was_left_alone_is_counted_by_reason_and_not_by_one_word() {
    // The defect this pins: one sentence for four states. A reader who
    // deleted a cover to see what would happen was told "every album already
    // has a cover, or none has been identified yet" — and had no way to learn
    // that the image was inside the files all along.
    let dir = sandbox("survey");

    let inside = survey(&library(&dir, true, None), &Sources::default(), false);
    assert_eq!((inside.embedded, inside.beside), (1, 0));
    assert!(inside.targets.is_empty());

    let alongside = survey(
        &library(&dir, false, Some("cover.jpg")),
        &Sources::default(),
        false,
    );
    assert_eq!((alongside.embedded, alongside.beside), (0, 1));

    // No cover and nothing naming it: a different state again, and the one
    // whose answer is "run fetch first".
    let folder = dir.join("music/Miles Davis/Kind of Blue");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    let unknown = build(
        vec![ScannedFile {
            path: folder.join("01.flac").to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        }],
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
    );
    let nameless = survey(&unknown, &Sources::default(), false);
    assert_eq!(nameless.unidentified, 1);
    assert_eq!(
        (nameless.embedded, nameless.beside, nameless.asked),
        (0, 0, 0)
    );

    // And asked-already is the fourth, which is why a second run is quiet.
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).expect("a release");
    layer.set(SourceRecord {
        key: entity.key.clone(),
        source: coverart::SOURCE.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts::default()),
    });
    let again = survey(&catalog, &layer, false);
    assert_eq!(again.asked, 1);
    assert!(again.targets.is_empty());
}

#[test]
fn an_album_whose_artwork_is_inside_its_files_is_told_where_to_go() {
    // `aede extract` was in the help and in the README and still could not be
    // found, because it was named nowhere near the moment somebody needs it:
    // delete a cover, run this, be told the image is inside the files, and
    // that is the end of the road. A command named only where nobody is
    // looking is a command nobody has.
    //
    // For one release this command ran the extraction itself instead. That was
    // reverted: two commands that both wrote, one of them not named for it,
    // cost more to hold in the head than the duplicated request it saved.
    let dir = sandbox("handover");
    let inside = survey(&library(&dir, true, None), &Sources::default(), false);
    let lines = reasons(&inside);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("aede extract"),
        "the way forward is named on the line that gives up: {}",
        lines[0]
    );
    // And the count leads the sentence, so the verb follows it.
    assert!(lines[0].starts_with("1 album carries"), "{}", lines[0]);

    let two = library(&dir, true, None);
    let mut both = survey(&two, &Sources::default(), false);
    both.embedded = 2;
    assert!(reasons(&both)[0].starts_with("2 albums carry"));
}

#[test]
fn a_cover_deleted_since_it_was_fetched_comes_back_from_the_stored_address() {
    // What sent a reader looking for `sources --forget`: they deleted a cover,
    // ran this, and were told the album had been asked about already. It had —
    // and the answer, an address, was sitting in the layer. A question that
    // was answered with a picture is not finished while the picture is gone.
    let dir = sandbox("again");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).expect("a release");
    layer.set(SourceRecord {
        key: entity.key.clone(),
        source: coverart::SOURCE.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            cover_art: Some("https://coverartarchive.org/x/1-1200.jpg".to_string()),
            ..Default::default()
        }),
    });

    let survey = survey(&catalog, &layer, false);
    assert_eq!(survey.asked, 0, "not a finished question");
    assert_eq!(survey.targets.len(), 1);
    assert!(survey.targets[0].known, "the address is the image");

    let mut transport = Canned::new(vec![], vec![Ok(JPEG.to_vec())]);
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        false,
        false,
    )
    .expect("the pass ran");

    assert_eq!(
        transport.asked,
        vec!["https://coverartarchive.org/x/1-1200.jpg"],
        "one request, and no index: the service is asked nothing it has \
         already answered"
    );
    assert_eq!(
        std::fs::read(cover_in(&dir)).expect("a cover on disk"),
        JPEG
    );
}

#[test]
fn an_album_the_archive_had_nothing_for_stays_a_finished_question() {
    // The other half of the same rule. Without it, every album with no artwork
    // anywhere would be asked about on every single run for ever.
    let dir = sandbox("nothing_finished");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).expect("a release");
    layer.set(SourceRecord {
        key: entity.key.clone(),
        source: coverart::SOURCE.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts::default()),
    });
    let survey = survey(&catalog, &layer, false);
    assert_eq!((survey.asked, survey.targets.len()), (1, 0));
}

#[test]
fn the_index_is_asked_first_and_the_image_second() {
    let dir = sandbox("walk");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(vec![Ok(INDEX.to_string())], vec![Ok(JPEG.to_vec())]);

    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        false,
        false,
    )
    .expect("the pass ran");

    assert_eq!(
        transport.asked,
        vec![
            // The edition the shelf holds, not the one somebody thought
            // representative: a reissue often has other artwork.
            "https://coverartarchive.org/release/59211ea4",
            "https://coverartarchive.org/x/1-1200.jpg",
        ]
    );
    assert_eq!(
        std::fs::read(cover_in(&dir)).expect("a cover on disk"),
        JPEG,
        "written beside the music, under the name the scanner looks for first"
    );
}

#[test]
fn bytes_that_are_not_an_image_are_not_written() {
    // The guard that matters most here. A download that went wrong — an error
    // page, a redirect gone astray, a truncated transfer — arrives as bytes
    // like any other, and writing them as `cover.jpg` is silent corruption
    // that surfaces months later when something tries to display it.
    let dir = sandbox("not_an_image");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(
        vec![Ok(INDEX.to_string())],
        vec![Ok(b"<!DOCTYPE html><html>Not Found".to_vec())],
    );

    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        false,
        false,
    )
    .expect("a refusal is reported, not fatal");

    assert!(
        !cover_in(&dir).exists(),
        "nothing was written into the music folder"
    );
    assert!(
        layer.records.is_empty(),
        "and nothing was recorded either, so the next run tries again"
    );
}

#[test]
fn a_file_that_appeared_since_the_scan_is_left_alone() {
    // The catalog is a snapshot; the disk is not. This is the one moment in
    // the program where being out of date destroys something the user made.
    let dir = sandbox("race");
    let catalog = library(&dir, false, None);
    std::fs::write(cover_in(&dir), b"mine, chosen by hand").expect("a cover put there by hand");

    let mut layer = Sources::default();
    let mut transport = Canned::new(vec![Ok(INDEX.to_string())], vec![Ok(JPEG.to_vec())]);
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        false,
        false,
    )
    .expect("a refusal is reported, not fatal");

    assert_eq!(
        std::fs::read(cover_in(&dir)).expect("still there"),
        b"mine, chosen by hand",
        "what was already there is what is still there"
    );
}

#[test]
fn a_record_with_no_artwork_is_recorded_so_it_is_not_asked_about_twice() {
    let dir = sandbox("nothing");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(vec![Ok(r#"{"images":[]}"#.to_string())], vec![]);
    let path = sources::sources_path(&dir);

    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &path,
        Size::Thumbnail(1200),
        false,
        false,
    )
    .expect("the pass ran");
    assert_eq!(transport.asked.len(), 1, "no image, so nothing downloaded");
    assert!(!cover_in(&dir).exists());
    assert_eq!(
        waiting(&catalog, &layer),
        0,
        "asked and answered — the next run costs nothing"
    );
}

#[test]
fn the_size_asked_for_is_the_one_downloaded_when_it_exists() {
    let dir = sandbox("size");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(vec![Ok(INDEX.to_string())], vec![Ok(JPEG.to_vec())]);
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(500),
        false,
        false,
    )
    .expect("the pass ran");
    assert_eq!(
        transport.asked[1],
        "https://coverartarchive.org/x/1-500.jpg"
    );

    // And the original, for a caller that asked for it.
    let dir = sandbox("size_original");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(vec![Ok(INDEX.to_string())], vec![Ok(JPEG.to_vec())]);
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Original,
        false,
        false,
    )
    .expect("the pass ran");
    assert_eq!(transport.asked[1], "https://coverartarchive.org/x/1.jpg");
}

#[test]
fn a_dry_run_asks_nothing_and_writes_nothing() {
    let dir = sandbox("dry");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(vec![], vec![]);
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        false,
        true,
    )
    .expect("the pass ran");
    assert!(transport.asked.is_empty());
    assert!(!cover_in(&dir).exists());
    assert!(layer.records.is_empty());
}

#[test]
fn an_album_nothing_has_identified_cannot_be_asked_about() {
    let dir = sandbox("unknown");
    let folder = dir.join("music/Miles Davis/Kind of Blue");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    let catalog = build(
        vec![ScannedFile {
            path: folder.join("01.flac").to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
        }],
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
    );
    assert_eq!(
        waiting(&catalog, &Sources::default()),
        0,
        "no identifier in the tags and nothing fetched: no question to ask"
    );

    // Once MusicBrainz has answered about the album, the group identifier is
    // the way in.
    let mut layer = Sources::default();
    let entity = EntityRef::of(&catalog, EntityKind::Release, 0).expect("a release");
    layer.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("c9fdb94c".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts::default()),
    });
    assert_eq!(waiting(&catalog, &layer), 1);
    assert_eq!(
        targets(&catalog, &layer)[0].url,
        "https://coverartarchive.org/release-group/c9fdb94c"
    );
}

/// An index with a front, a back and two booklet pages: what `--images` is for.
const INDEX_ALL: &str = r#"{"images":[
    {"front":true,"approved":true,"image":"https://x/1.jpg",
     "thumbnails":{"1200":"https://x/1-1200.jpg"}},
    {"types":["Back"],"image":"https://x/2.jpg",
     "thumbnails":{"1200":"https://x/2-1200.jpg"}},
    {"types":["Booklet"],"image":"https://x/3.jpg",
     "thumbnails":{"1200":"https://x/3-1200.jpg"}},
    {"types":["Booklet"],"image":"https://x/4.jpg",
     "thumbnails":{"1200":"https://x/4-1200.jpg"}}]}"#;

#[test]
fn with_images_an_album_that_has_a_cover_is_asked_about_once() {
    // The cover is one question and the booklet is another, and `--images`
    // asks the second of an album finished for the first. What stops it being
    // asked for ever after is the `artwork/` folder itself — the same way
    // `cover.jpg` is what stops the ordinary pass.
    let dir = sandbox("images_scope");
    let catalog = library(&dir, false, Some("cover.jpg"));

    let plain = survey(&catalog, &Sources::default(), false);
    assert!(plain.targets.is_empty());
    assert_eq!(plain.beside, 1);

    let wider = survey(&catalog, &Sources::default(), true);
    assert_eq!(wider.targets.len(), 1);
    assert!(
        !wider.targets[0].cover,
        "the cover is there; only the rest is missing"
    );

    std::fs::create_dir_all(dir.join("music/Miles Davis/Kind of Blue/artwork")).expect("a folder");
    let done = survey(&catalog, &Sources::default(), true);
    assert!(done.targets.is_empty(), "the folder is the record");
    assert_eq!(done.beside, 1);
}

#[test]
fn with_images_a_stored_address_is_not_enough_and_the_index_is_asked() {
    // The stored address is the front image. It says nothing about the back,
    // so the one-request shortcut is no use here and the index is fetched.
    let dir = sandbox("images_known");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let entity = aede_core::user::EntityRef::of(
        &catalog,
        aede_core::model::EntityKind::Release,
        catalog.releases[0].id,
    )
    .expect("an album with an identifier");
    layer.set(SourceRecord {
        key: entity.key.clone(),
        source: coverart::SOURCE.to_string(),
        source_id: None,
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Release(ReleaseFacts {
            cover_art: Some("https://x/1-1200.jpg".to_string()),
            ..Default::default()
        }),
    });

    assert!(survey(&catalog, &layer, false).targets[0].known);
    let wider = survey(&catalog, &layer, true);
    assert!(!wider.targets[0].known, "the index has to be asked again");
    assert!(
        wider.targets[0]
            .url
            .starts_with("https://coverartarchive.org/")
    );
}

#[test]
fn the_cover_stays_beside_the_music_and_the_rest_goes_one_level_down() {
    // The point of the whole option. A `back.jpg` next to the tracks would be
    // taken for the album's cover by this program's own scanner, which is the
    // wrong picture and one nothing would look at again.
    let dir = sandbox("images_run");
    let catalog = library(&dir, false, None);
    let mut layer = Sources::default();
    let mut transport = Canned::new(
        vec![Ok(INDEX_ALL.to_string())],
        vec![
            Ok(JPEG.to_vec()),
            Ok(JPEG.to_vec()),
            Ok(JPEG.to_vec()),
            Ok(JPEG.to_vec()),
        ],
    );
    run(
        &catalog,
        &mut transport,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        true,
        false,
    )
    .expect("the pass ran");

    let folder = dir.join("music/Miles Davis/Kind of Blue");
    let into = coverart::extras_in(&folder);
    assert!(cover_in(&dir).exists(), "the cover, where everything looks");
    for name in ["back.jpg", "booklet-01.jpg", "booklet-02.jpg"] {
        assert!(into.join(name).exists(), "missing: {name}");
        assert!(
            !folder.join(name).exists(),
            "{name} must not sit beside the music"
        );
    }

    assert_eq!(
        transport.asked,
        vec![
            "https://coverartarchive.org/release/59211ea4",
            "https://x/2-1200.jpg",
            "https://x/3-1200.jpg",
            "https://x/4-1200.jpg",
            "https://x/1-1200.jpg",
        ],
        "one index, then every image it named, the front last"
    );

    // Once the disk has been read again there is nothing left to ask about:
    // `cover.jpg` answers for the front and `artwork/` for the rest. It takes
    // a scan, because the catalog is a snapshot and the file this pass just
    // wrote is not in it until something looks.
    let rescanned = library(&dir, false, Some("cover.jpg"));
    assert!(
        survey(&rescanned, &layer, true).targets.is_empty(),
        "the artwork folder is there now, and the cover is too"
    );

    // And a second run before that scan writes over nothing: every image is
    // already on disk, and a file already there is not a failure.
    let mut again = Canned::new(
        vec![Ok(INDEX_ALL.to_string())],
        vec![
            Ok(JPEG.to_vec()),
            Ok(JPEG.to_vec()),
            Ok(JPEG.to_vec()),
            Ok(JPEG.to_vec()),
        ],
    );
    run(
        &catalog,
        &mut again,
        &[],
        &mut layer,
        &sources::sources_path(&dir),
        Size::Thumbnail(1200),
        true,
        false,
    )
    .expect("the pass ran");
    assert_eq!(
        std::fs::read_dir(&into).expect("readable").count(),
        3,
        "the same three images, not six"
    );
}
