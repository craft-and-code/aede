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
use aede_core::store;
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
fn downloaded_portraits_are_validated_and_existing_pictures_stay_unchanged() {
    let dir = sandbox("validated_portrait");
    let target = Target {
        entity: entity(),
        name: "Miles Davis".into(),
        mbid: "mbid-1".into(),
        wikidata_id: Some("Q11649".into()),
        fanarttv_mbid: Some("mbid-1".into()),
        destination: dir.join("artist"),
    };
    let png = include_bytes!("../../../aede-core/tests/fixtures/images/rgba.png");
    assert!(matches!(
        write(&target, &png[..png.len() - 12]),
        Err(Refusal::Failed(_))
    ));
    assert!(!target.destination.exists());
    assert!(write(&target, png).expect("valid portrait"));
    assert!(!write(&target, png).expect("already present"));
    assert!(write(&target, &png[..png.len() - 12]).is_err());
    assert_eq!(
        std::fs::read(target.destination.join("artist.png")).expect("preserved"),
        png
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_portrait_answer_for_another_artist_is_not_downloaded_or_cached() {
    struct WrongIdentity {
        downloads: usize,
    }
    impl Ask for WrongIdentity {
        fn get_json(&mut self, _url: &str) -> Result<aede_core::json::Json, Refusal> {
            Ok(aede_core::json::parse(r#"{"mbid":"another-artist"}"#).unwrap())
        }
        fn get_bytes(&mut self, _url: &str) -> Result<Vec<u8>, Refusal> {
            self.downloads += 1;
            Err(Refusal::Failed("unexpected download".into()))
        }
    }
    let dir = sandbox("wrong_identity");
    let catalog = one_album(&dir);
    let layer = held(None);
    let selected = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        Some("key"),
        false,
    );
    let target = &selected[0];
    let mut transport = WrongIdentity { downloads: 0 };
    assert!(attempt(&mut transport, &[], target, Some("key")).is_err());
    assert_eq!(transport.downloads, 0);
    assert!(layer.get(&entity(), fanarttv::SOURCE).is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn path_shaped_source_identifiers_cannot_choose_a_portrait_destination() {
    struct PortraitAnswer {
        requested: Vec<String>,
    }
    impl Ask for PortraitAnswer {
        fn get_json(&mut self, url: &str) -> Result<aede_core::json::Json, Refusal> {
            self.requested.push(url.to_string());
            Ok(aede_core::json::parse(
                r#"{"entities":{"Q11649":{"claims":{"P18":[{"rank":"normal","mainsnak":{"snaktype":"value","datavalue":{"value":"Miles_Davis.jpg"}}}]}}}}"#,
            )
            .unwrap())
        }
        fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
            self.requested.push(url.to_string());
            Ok(include_bytes!("../../../aede-core/tests/fixtures/images/rgba.png").to_vec())
        }
    }

    let dir = sandbox("unsafe_asset_identifier");
    let data = dir.join("data");
    let outside = dir.join("outside");
    let catalog = scattered_albums(&dir);
    for file in &catalog.files {
        let path = std::path::Path::new(&file.path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"original audio remains unchanged").unwrap();
    }
    for mbid in [
        outside.to_str().unwrap(),
        "../outside",
        r"..\outside",
        ".",
        "..",
        "",
        "nul\0identifier",
    ] {
        let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
        layer.records[0].source_id = Some(mbid.into());
        let before = sources::to_json(&layer);
        let mut transport = PortraitAnswer { requested: vec![] };
        run(
            &catalog,
            &mut transport,
            &[],
            &mut layer,
            &sources::sources_path(&data),
            &data,
            None,
            &crate::commands::fetch::Asked {
                names: &[],
                scope: &crate::commands::fetch::EVERYTHING,
                again: false,
                dry_run: false,
                size: crate::commands::covers::DEFAULT_SIZE,
                size_requested: false,
                images: false,
                fanart: Default::default(),
                langs: vec!["en".into()],
                key: None,
                portrait_key: None,
            },
        )
        .unwrap();
        assert!(
            !outside.exists(),
            "an imported source identifier must not create an outside destination"
        );
        assert!(transport.requested.is_empty(), "invalid path {mbid:?}");
        assert_eq!(sources::to_json(&layer), before);
    }
    assert!(!data.exists());
    for file in &catalog.files {
        assert_eq!(
            std::fs::read(&file.path).unwrap(),
            b"original audio remains unchanged"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
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
        source_id: Some("Q11649".to_string()),
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
        source_id: Some("Q11649".to_string()),
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
    assert!(!has_portrait(&layer, &entity(), "mbid-1", None));

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
        has_portrait(&layer, &entity(), "mbid-1", None),
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

fn portrait_asked(again: bool) -> crate::commands::fetch::Asked<'static> {
    crate::commands::fetch::Asked {
        names: &[],
        scope: &crate::commands::fetch::EVERYTHING,
        again,
        dry_run: false,
        size: crate::commands::covers::DEFAULT_SIZE,
        size_requested: false,
        images: false,
        fanart: Default::default(),
        langs: vec!["en".into()],
        key: None,
        portrait_key: None,
    }
}

struct PortraitMetadata {
    doc: aede_core::json::Json,
    requested: Vec<String>,
}

#[test]
fn correcting_the_wikidata_link_reopens_a_portrait_lookup_for_the_same_artist() {
    for picture in [false, true] {
        let dir = sandbox(&format!("changed_wikidata_link_{picture}"));
        let catalog = one_album(&dir);
        let mut layer = held(Some("https://www.wikidata.org/wiki/Q1001"));
        let path = sources::sources_path(&dir);
        let mut transport = PortraitMetadata {
            doc: aede_core::json::parse(r#"{"entities":{"Q1001":{"claims":{}}}}"#).unwrap(),
            requested: Vec::new(),
        };
        run(
            &catalog,
            &mut transport,
            &[],
            &mut layer,
            &path,
            &dir,
            None,
            &portrait_asked(false),
        )
        .unwrap();
        assert_eq!(transport.requested.len(), 1);
        layer = sources::load_all(&path).unwrap().unwrap();
        if picture {
            let record = layer
                .records
                .iter_mut()
                .find(|record| record.source == wikipedia::PORTRAIT_SOURCE)
                .unwrap();
            let Facts::Artist(facts) = &mut record.facts else {
                panic!("portrait facts")
            };
            facts.portrait = Some(Picture {
                url: "https://retained.example/portrait.png".into(),
            });
        }
        let record = layer
            .records
            .iter_mut()
            .find(|record| record.source == sources::MUSICBRAINZ)
            .unwrap();
        let Facts::Artist(facts) = &mut record.facts else {
            panic!("artist facts")
        };
        facts.wikidata = Some("https://www.wikidata.org/wiki/Q1002".into());
        sources::save(&layer, &path).unwrap();
        layer = sources::load_all(&path).unwrap().unwrap();
        transport.doc = aede_core::json::parse(r#"{"entities":{"Q1002":{"claims":{}}}}"#).unwrap();
        for _ in 0..2 {
            run(
                &catalog,
                &mut transport,
                &[],
                &mut layer,
                &path,
                &dir,
                None,
                &portrait_asked(false),
            )
            .unwrap();
            assert_eq!(
                transport.requested.len(),
                2,
                "a cached portrait or absence for Q1001 must not complete Q1002"
            );
            assert_eq!(transport.requested[1], wikipedia::entity_data_url("Q1002"));
            layer = sources::load_all(&path).unwrap().unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn legacy_portrait_receipts_are_confirmed_once_without_overwriting_existing_images() {
    for picture in [false, true] {
        for on_disk in [false, true] {
            let dir = sandbox(&format!("legacy_wikidata_{picture}_{on_disk}"));
            let catalog = one_album(&dir);
            let mut layer = held(Some("https://www.wikidata.org/wiki/Q1001"));
            let destination = targets(
                &catalog,
                &layer,
                &[],
                &super::super::fetch::EVERYTHING,
                &dir,
                None,
                false,
            )
            .remove(0)
            .destination;
            let png = include_bytes!("../../../aede-core/tests/fixtures/images/rgba.png");
            if on_disk {
                coverart::write_image(&destination, Kind::Artist, (0, 1), png).unwrap();
            }
            layer.set(SourceRecord {
                key: entity().key,
                source: wikipedia::PORTRAIT_SOURCE.into(),
                source_id: Some("mbid-1".into()),
                fetched_at: 1,
                confidence: Confidence::Identified,
                facts: Facts::Artist(ArtistFacts {
                    portrait: picture.then(|| Picture {
                        url: "https://retained.example/portrait.png".into(),
                    }),
                    ..Default::default()
                }),
            });
            let path = sources::sources_path(&dir);
            sources::save(&layer, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let mut transport = PortraitMetadata {
                doc: aede_core::json::parse("{}").unwrap(),
                requested: Vec::new(),
            };
            run(
                &catalog,
                &mut transport,
                &[],
                &mut layer,
                &path,
                &dir,
                None,
                &portrait_asked(false),
            )
            .unwrap();
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "a failed confirmation retains legacy evidence"
            );
            assert_eq!(transport.requested.len(), usize::from(!on_disk));
            transport.doc =
                aede_core::json::parse(r#"{"entities":{"Q1001":{"claims":{}}}}"#).unwrap();
            for _ in 0..2 {
                layer = sources::load_all(&path).unwrap().unwrap();
                run(
                    &catalog,
                    &mut transport,
                    &[],
                    &mut layer,
                    &path,
                    &dir,
                    None,
                    &portrait_asked(false),
                )
                .unwrap();
                assert_eq!(transport.requested.len(), if on_disk { 0 } else { 2 });
            }
            if on_disk {
                assert_eq!(std::fs::read(destination.join("artist.png")).unwrap(), png);
                assert_eq!(std::fs::read(&path).unwrap(), before);
            } else {
                let saved = sources::load_all(&path).unwrap().unwrap();
                assert_eq!(
                    saved
                        .get(&entity(), wikipedia::PORTRAIT_SOURCE)
                        .unwrap()
                        .source_id
                        .as_deref(),
                    Some("Q1001")
                );
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn changed_artwork_identity_reopens_portrait_answers_without_repeating_current_ones() {
    for wikidata in [false, true] {
        for picture in [false, true] {
            for changed in [false, true] {
                let dir = sandbox(&format!("changed_identity_{wikidata}_{picture}_{changed}"));
                let catalog = one_album(&dir);
                let mut layer = held(wikidata.then_some("https://www.wikidata.org/wiki/Q11649"));
                layer.records[0].source_id = Some("11111111-1111-4111-8111-111111111111".into());
                layer.set(SourceRecord {
                    key: entity().key,
                    source: if wikidata {
                        wikipedia::PORTRAIT_SOURCE
                    } else {
                        fanarttv::SOURCE
                    }
                    .into(),
                    source_id: Some(
                        if wikidata {
                            if changed { "Q1001" } else { "Q11649" }
                        } else if changed {
                            "44444444-4444-4444-8444-444444444444"
                        } else {
                            "11111111-1111-4111-8111-111111111111"
                        }
                        .into(),
                    ),
                    fetched_at: 1,
                    confidence: Confidence::Identified,
                    facts: Facts::Artist(ArtistFacts {
                        portrait: picture.then(|| Picture {
                            url: "https://retained.example/portrait.png".into(),
                        }),
                        ..Default::default()
                    }),
                });
                let path = sources::sources_path(&dir);
                sources::save(&layer, &path).unwrap();
                let mut transport = PortraitMetadata {
                    doc: aede_core::json::parse(if wikidata {
                        r#"{"entities":{"Q11649":{"claims":{}}}}"#
                    } else {
                        r#"{"mbid_id":"11111111-1111-4111-8111-111111111111","artistthumb":[]}"#
                    })
                    .unwrap(),
                    requested: Vec::new(),
                };
                for _ in 0..2 {
                    run(
                        &catalog,
                        &mut transport,
                        &[],
                        &mut layer,
                        &path,
                        &dir,
                        (!wikidata).then_some("fixture-key"),
                        &portrait_asked(false),
                    )
                    .unwrap();
                    assert_eq!(
                        transport.requested.len(),
                        usize::from(changed),
                        "wikidata {wikidata}, picture {picture}, changed {changed}"
                    );
                    layer = sources::load_all(&path).unwrap().unwrap();
                }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

impl Ask for PortraitMetadata {
    fn get_json(&mut self, url: &str) -> Result<aede_core::json::Json, Refusal> {
        self.requested.push(url.into());
        Ok(self.doc.clone())
    }

    fn get_bytes(&mut self, url: &str) -> Result<Vec<u8>, Refusal> {
        self.requested.push(url.into());
        Ok(include_bytes!("../../../aede-core/tests/fixtures/images/rgba.png").to_vec())
    }
}

#[test]
fn malformed_portrait_metadata_preserves_answers_and_ordinary_retry() {
    for (case, (wikidata, invalid)) in [
        (true, r#"{}"#),
        (true, r#"{"entities":[]}"#),
        (true, r#"{"entities":{"Q11649":null}}"#),
        (true, r#"{"entities":{"Q11649":{"claims":[]}}}"#),
        (true, r#"{"entities":{"Q11649":{"claims":{"P18":{}}}}}"#),
        (false, r#"{"mbid_id":"mbid-1","artistthumb":{}}"#),
    ]
    .into_iter()
    .enumerate()
    {
        for refresh in [false, true] {
            let dir = sandbox(&format!("bad_metadata_{case}_{refresh}"));
            let catalog = one_album(&dir);
            let mut layer = held(wikidata.then_some("https://www.wikidata.org/wiki/Q11649"));
            let source = if wikidata {
                wikipedia::PORTRAIT_SOURCE
            } else {
                fanarttv::SOURCE
            };
            let key = (!wikidata).then_some("fictitious-key");
            if refresh {
                layer.set(SourceRecord {
                    key: entity().key,
                    source: source.into(),
                    source_id: Some("mbid-1".into()),
                    fetched_at: 7,
                    confidence: Confidence::Identified,
                    facts: Facts::Artist(ArtistFacts {
                        portrait: Some(Picture {
                            url: "https://retained.example/credited.jpg".into(),
                        }),
                        ..Default::default()
                    }),
                });
            }
            let path = sources::sources_path(&dir);
            sources::save(&layer, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let before_layer = sources::to_json(&layer);
            let mut transport = PortraitMetadata {
                doc: aede_core::json::parse(invalid).unwrap(),
                requested: vec![],
            };
            run(
                &catalog,
                &mut transport,
                &[],
                &mut layer,
                &path,
                &dir,
                key,
                &portrait_asked(refresh),
            )
            .unwrap();
            assert_eq!(transport.requested.len(), 1, "{invalid}");
            assert_eq!(sources::to_json(&layer), before_layer, "{invalid}");
            assert_eq!(std::fs::read(&path).unwrap(), before, "{invalid}");
            layer = sources::load_all(&path).unwrap().unwrap();
            if !refresh {
                assert_eq!(
                    waiting(&catalog, &layer, &dir, key),
                    1,
                    "invalid metadata stays retryable"
                );
            }

            // Missing optional claims and an empty image pool are genuine absences.
            transport.doc = aede_core::json::parse(if wikidata {
                r#"{"entities":{"Q11649":{"sitelinks":{}}}}"#
            } else {
                r#"{"mbid_id":"mbid-1","artistthumb":[]}"#
            })
            .unwrap();
            run(
                &catalog,
                &mut transport,
                &[],
                &mut layer,
                &path,
                &dir,
                key,
                &portrait_asked(refresh),
            )
            .unwrap();
            assert_eq!(transport.requested.len(), 2);
            layer = sources::load_all(&path).unwrap().unwrap();
            assert!(layer.get(&entity(), source).is_some());
            assert_eq!(
                waiting(&catalog, &layer, &dir, key),
                0,
                "a valid absence is completed"
            );
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn a_portrait_already_on_disk_is_neither_downloaded_nor_attributed_to_a_provider() {
    for (extension, image) in [
        (
            "jpg",
            include_bytes!("../../../aede-core/tests/fixtures/images/baseline.jpg").as_slice(),
        ),
        (
            "png",
            include_bytes!("../../../aede-core/tests/fixtures/images/rgba.png").as_slice(),
        ),
    ] {
        for again in [false, true] {
            let dir = sandbox(&format!("existing_{extension}_{again}"));
            let catalog = one_album(&dir);
            let mut layer = held(Some("https://www.wikidata.org/wiki/Q11649"));
            let path = sources::sources_path(&dir);
            sources::save(&layer, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let destination = dir.join("music/Miles Davis");
            std::fs::create_dir_all(&destination).unwrap();
            std::fs::write(destination.join(format!("artist.{extension}")), image).unwrap();
            let mut transport = PortraitMetadata {
                doc: aede_core::json::parse(r#"{"entities":{"Q11649":{"claims":{"P18":[{"mainsnak":{"datavalue":{"value":"Provider.png"}}}]}}}}"#).unwrap(),
                requested: vec![],
            };
            run(
                &catalog,
                &mut transport,
                &[],
                &mut layer,
                &path,
                &dir,
                Some("fictitious-key"),
                &portrait_asked(again),
            )
            .unwrap();
            assert!(
                transport.requested.is_empty(),
                "an existing {extension} portrait is not asked about"
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(
                std::fs::read(destination.join(format!("artist.{extension}"))).unwrap(),
                image
            );
            assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 1);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
