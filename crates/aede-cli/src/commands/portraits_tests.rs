//! What the portraits pass decides, proved without a network.
//!
//! Declared in `portraits.rs` with `#[path]`, so this is still that module's
//! own child and still reaches its private items through `use super::*`.
//!
//! The network walk itself — `run`, `attempt` — is exercised only lightly
//! here; what is proven in depth is the part most likely to be wrong and
//! least likely to be caught by reading it twice: which artist is even a
//! target, which folder a picture is written into, and what gets recorded
//! for "nothing found".

use super::*;
use aede_core::model::builder::{ScannedFile, build};
use aede_core::sources::{ArtistFacts, Confidence, Sources};
use aede_core::tags::RawTags;

/// The test that owns this folder, for a name no other test can produce —
/// see `covers_tests.rs` for why both halves of the name are needed.
fn owner() -> String {
    std::thread::current()
        .name()
        .map(|name| name.replace("::", "_"))
        .unwrap_or_else(|| "main".to_string())
}

fn sandbox(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aede_portraits_{}_{name}", owner()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a data folder");
    dir
}

/// A one-artist, one-album library, with the album under `dir/music`.
fn one_album(dir: &std::path::Path) -> Catalog {
    let folder = dir.join("music/Miles Davis/Kind of Blue");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    build(
        vec![ScannedFile {
            path: folder.join("01.flac").to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
        &[],
    )
}

/// The same artist, with two albums that do not share a folder: one under
/// the watched root directly, one nested one level deeper than the other —
/// nothing a shared parent can be found for.
fn scattered_albums(dir: &std::path::Path) -> Catalog {
    let mut files = Vec::new();
    for (album, rel) in [
        ("Kind of Blue", "music/Kind of Blue"),
        ("Bitches Brew", "music/1970s/Bitches Brew"),
    ] {
        let folder = dir.join(rel);
        let mut tags = RawTags::default();
        tags.insert("artist", "Miles Davis");
        tags.insert("albumartist", "Miles Davis");
        tags.insert("album", album);
        tags.insert("title", "A track");
        files.push(ScannedFile {
            path: folder.join("01.flac").to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        });
    }
    build(
        files,
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
        &[],
    )
}

/// A layer holding one MusicBrainz artist record for "miles davis".
fn held(wikidata: Option<&str>) -> Sources {
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "miles davis".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            wikidata: wikidata.map(str::to_string),
            ..Default::default()
        }),
    });
    sources
}

fn entity() -> EntityRef {
    EntityRef {
        kind: aede_core::model::EntityKind::Artist,
        key: "miles davis".to_string(),
    }
}

#[test]
fn a_single_shared_folder_is_the_destination() {
    let dir = sandbox("shared_folder");
    let catalog = one_album(&dir);
    let list = targets(
        &catalog,
        &held(None),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].destination,
        dir.join("music/Miles Davis"),
        "beside the music, in the folder every one of the artist's albums shares"
    );
}

#[test]
fn albums_that_share_no_folder_fall_back_to_assets() {
    let dir = sandbox("scattered");
    let catalog = scattered_albums(&dir);
    let list = targets(
        &catalog,
        &held(None),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].destination,
        store::assets_dir(&dir).join("artists").join("mbid-1"),
        "no shared parent to write beside, so the per-artist assets/ folder, \
         named by the identifier that can never collide"
    );
}

#[test]
fn a_watched_root_is_never_treated_as_an_artist_folder() {
    // An artist with exactly one album sitting directly under the watched
    // root has a "shared folder" that is the root itself — not a place to
    // drop a picture for one artist among everyone else's.
    let dir = sandbox("root_is_shared");
    // A catalog whose one album sits directly under the watched root, so its
    // "shared folder" is the root itself.
    let folder = dir.join("music/Kind of Blue");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    let root_only = build(
        vec![ScannedFile {
            path: folder.join("01.flac").to_string_lossy().to_string(),
            size: 1,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec![dir.join("music").to_string_lossy().to_string()],
        1,
        &[],
    );
    let list = targets(
        &root_only,
        &held(None),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].destination,
        store::assets_dir(&dir).join("artists").join("mbid-1"),
        "the root is not this artist's folder, so it falls back to assets/"
    );
}

#[test]
fn wikidata_is_tried_when_linked_and_fanarttv_needs_a_key() {
    let dir = sandbox("eligibility");
    let catalog = one_album(&dir);

    // No wikidata link, no fanart.tv key: nothing this pass could ask.
    let none = targets(
        &catalog,
        &held(None),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        None,
        false,
    );
    assert!(none.is_empty(), "nothing to ask with is not a target");

    // A wikidata link but no key: wikidata alone is worth trying.
    let wiki_only = targets(
        &catalog,
        &held(Some("https://www.wikidata.org/wiki/Q11649")),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        None,
        false,
    );
    assert_eq!(wiki_only.len(), 1);
    assert_eq!(wiki_only[0].wikidata_id.as_deref(), Some("Q11649"));
    assert!(
        wiki_only[0].fanarttv_mbid.is_none(),
        "no key, so not eligible"
    );

    // No wikidata link but a key: fanart.tv alone is worth trying.
    let fanarttv_only = targets(
        &catalog,
        &held(None),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    assert_eq!(fanarttv_only.len(), 1);
    assert!(fanarttv_only[0].wikidata_id.is_none());
    assert_eq!(fanarttv_only[0].fanarttv_mbid.as_deref(), Some("mbid-1"));
}

#[test]
fn an_artist_with_a_stored_picture_is_not_a_target_again() {
    let dir = sandbox("has_picture");
    let catalog = one_album(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: wikipedia::PORTRAIT_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            portrait: Some(Picture {
                url: "https://commons.wikimedia.org/x.jpg".to_string(),
            }),
            ..Default::default()
        }),
    });
    let list = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    assert!(list.is_empty(), "a picture is already on record");

    let again = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        true,
    );
    assert_eq!(
        again.len(),
        1,
        "--full asks again even with a picture on file"
    );
}

#[test]
fn a_source_already_asked_and_empty_is_not_asked_again_but_the_other_still_is() {
    let dir = sandbox("half_asked");
    let catalog = one_album(&dir);
    let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
    // Wikidata already answered with nothing.
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: wikipedia::PORTRAIT_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts::default()),
    });
    let list = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    assert_eq!(
        list.len(),
        1,
        "fanart.tv has a key now and has never been asked"
    );
    assert!(
        list[0].wikidata_id.is_none(),
        "wikidata already answered — not asked twice"
    );
    assert_eq!(list[0].fanarttv_mbid.as_deref(), Some("mbid-1"));
}

#[test]
fn store_records_a_written_picture_under_the_source_that_found_it() {
    let dir = sandbox("store_written");
    let mut layer = Sources::default();
    let target = Target {
        entity: entity(),
        name: "Miles Davis".to_string(),
        wikidata_id: Some("Q11649".to_string()),
        fanarttv_mbid: None,
        mbid: "mbid-1".to_string(),
        destination: dir.join("Miles Davis"),
    };
    store(
        &mut layer,
        &target,
        &Outcome::Written {
            source: wikipedia::PORTRAIT_SOURCE,
            url: "https://commons.wikimedia.org/x.jpg".to_string(),
            new: true,
        },
    );
    let record = layer
        .get(&entity(), wikipedia::PORTRAIT_SOURCE)
        .expect("a record");
    let Facts::Artist(artist) = &record.facts else {
        panic!("an artist record")
    };
    assert_eq!(
        artist.portrait.as_ref().map(|p| p.url.as_str()),
        Some("https://commons.wikimedia.org/x.jpg")
    );
    assert!(
        layer.get(&entity(), fanarttv::SOURCE).is_none(),
        "fanart.tv was never asked, so it gets no record"
    );
}

#[test]
fn store_records_nothing_found_only_for_the_sources_actually_asked() {
    let dir = sandbox("store_nothing");
    let _ = &dir;
    let mut layer = Sources::default();
    let target = Target {
        entity: entity(),
        name: "Miles Davis".to_string(),
        wikidata_id: Some("Q11649".to_string()),
        fanarttv_mbid: Some("mbid-1".to_string()),
        mbid: "mbid-1".to_string(),
        destination: std::env::temp_dir().join("wherever"),
    };
    store(
        &mut layer,
        &target,
        &Outcome::Nothing {
            wikidata_asked: true,
            fanarttv_asked: true,
        },
    );
    for source in [wikipedia::PORTRAIT_SOURCE, fanarttv::SOURCE] {
        let record = layer.get(&entity(), source).expect("a record");
        let Facts::Artist(artist) = &record.facts else {
            panic!("an artist record")
        };
        assert!(artist.portrait.is_none(), "nothing was found from {source}");
    }
}

#[test]
fn has_portrait_reads_either_source() {
    let mut layer = Sources::default();
    assert!(!has_portrait(&layer, &entity()));

    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            portrait: Some(Picture {
                url: "https://fanart.tv/x.jpg".to_string(),
            }),
            ..Default::default()
        }),
    });
    assert!(
        has_portrait(&layer, &entity()),
        "a picture from either source counts"
    );
}

#[test]
fn what_fetch_offers_is_exactly_what_this_pass_would_ask() {
    let dir = sandbox("waiting");
    let catalog = one_album(&dir);
    assert_eq!(waiting(&catalog, &held(None), &dir, None), 0);
    assert_eq!(
        waiting(
            &catalog,
            &held(Some("https://www.wikidata.org/wiki/Q11649")),
            &dir,
            None
        ),
        1
    );
}
