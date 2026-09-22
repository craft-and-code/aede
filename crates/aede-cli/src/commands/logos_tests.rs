//! What the logos pass decides, proved without a network.
//!
//! Declared in `logos.rs` with `#[path]`, so this is still that module's own
//! child and still reaches its private items through `use super::*`.
//!
//! Mirrors `portraits_tests.rs` in shape, narrowed to one source: there is no
//! eligibility question to prove about a second source that does not exist
//! here, but there is a question `portraits_tests.rs` never had to ask —
//! whether writing a logo can clobber a portrait already on record for the
//! same artist, since both are answered by the same Fanart.tv lookup. See
//! `store_writing_a_logo_never_erases_a_portrait_already_on_record` below.

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
    let dir = std::env::temp_dir().join(format!("aede_logos_{}_{name}", owner()));
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
    tags.insert("musicbrainz_releasegroupid", "release-group-1");
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
fn held() -> Sources {
    let mut sources = Sources::default();
    sources.set(SourceRecord {
        key: "miles davis".to_string(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts::default()),
    });
    sources
}

fn entity() -> EntityRef {
    EntityRef {
        kind: aede_core::model::EntityKind::Artist,
        key: "miles davis".to_string(),
    }
}

fn logo_options() -> FanartOptions {
    FanartOptions {
        logo: true,
        label_logo: true,
        ..Default::default()
    }
}

fn banner_options() -> FanartOptions {
    FanartOptions {
        banner: true,
        ..logo_options()
    }
}

fn all_options() -> FanartOptions {
    FanartOptions {
        all: true,
        logo: true,
        label_logo: true,
        portrait: true,
        background: true,
        banner: true,
        album_cover: true,
        cdart: true,
    }
}

#[test]
fn a_single_shared_folder_is_the_destination() {
    let dir = sandbox("shared_folder");
    let catalog = one_album(&dir);
    let list = targets(
        &catalog,
        &held(),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        logo_options(),
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
        &held(),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        logo_options(),
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
    // drop a logo for one artist among everyone else's.
    let dir = sandbox("root_is_shared");
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
        &held(),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        logo_options(),
    );
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].destination,
        store::assets_dir(&dir).join("artists").join("mbid-1"),
        "the root is not this artist's folder, so it falls back to assets/"
    );
}

#[test]
fn an_artist_with_a_stored_logo_is_not_a_target_again() {
    let dir = sandbox("has_logo");
    let catalog = one_album(&dir);
    let mut layer = held();
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::LOGO_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            logo: Some(Picture {
                url: "https://assets.fanart.tv/fanart/music/miles-davis/logo.png".to_string(),
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
        false,
        logo_options(),
    );
    assert!(list.is_empty(), "a logo is already on record");

    let again = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        true,
        logo_options(),
    );
    assert_eq!(again.len(), 1, "--full asks again even with a logo on file");
}

#[test]
fn the_full_fanart_pass_revisits_an_old_logo_then_records_artist_and_album() {
    let dir = sandbox("fanart_completion");
    let catalog = one_album(&dir);
    let mut layer = held();
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::LOGO_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            logo: Some(Picture {
                url: "https://x/logo.png".to_string(),
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
        false,
        all_options(),
    );
    assert_eq!(list.len(), 1, "an old logo did not fetch the newer kinds");
    assert_eq!(list[0].albums.len(), 1);
    assert_eq!(list[0].albums[0].release_group, "release-group-1");

    store_artwork(&mut layer, &list[0], all_options());
    assert!(
        targets(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            &dir,
            false,
            all_options(),
        )
        .is_empty(),
        "a completed full-artwork lookup should not be repeated"
    );
}

#[test]
fn completing_one_fanart_family_does_not_complete_the_others() {
    let dir = sandbox("fanart_family_completion");
    let catalog = one_album(&dir);
    let mut layer = held();
    let background_only = FanartOptions {
        all: true,
        background: true,
        ..Default::default()
    };

    let list = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        background_only,
    );
    assert_eq!(list.len(), 1);
    store_artwork(&mut layer, &list[0], background_only);
    assert!(
        targets(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            &dir,
            false,
            background_only,
        )
        .is_empty(),
        "the selected family is complete"
    );

    let portrait_only = FanartOptions {
        all: true,
        portrait: true,
        ..Default::default()
    };
    assert_eq!(
        targets(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            &dir,
            false,
            portrait_only,
        )
        .len(),
        1,
        "an excluded family remains available for a later run"
    );
}

#[test]
fn an_artist_with_a_logo_but_no_banner_is_still_a_target_when_banners_are_asked_for() {
    // The logo alone satisfies an ordinary run — proved just above — but
    // `--banners` asks a second question about the very same folder, and a
    let dir = sandbox("has_logo_no_banner");
    let catalog = one_album(&dir);
    let mut layer = held();
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::LOGO_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            logo: Some(Picture {
                url: "https://assets.fanart.tv/fanart/music/miles-davis/logo.png".to_string(),
            }),
            ..Default::default()
        }),
    });

    let without_banners = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        logo_options(),
    );
    assert!(
        without_banners.is_empty(),
        "no --banners was asked for, so the logo alone is enough"
    );

    let with_banners = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        banner_options(),
    );
    assert_eq!(
        with_banners.len(),
        1,
        "the logo is on record, but no banner is on disk yet"
    );
}

#[test]
fn a_banner_already_on_disk_is_not_asked_about_again() {
    let dir = sandbox("has_banner_on_disk");
    let catalog = one_album(&dir);
    let mut layer = held();
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::LOGO_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            logo: Some(Picture {
                url: "https://assets.fanart.tv/fanart/music/miles-davis/logo.png".to_string(),
            }),
            ..Default::default()
        }),
    });
    // A banner tracked by disk presence alone — see the module doc — so
    // putting the file there, with no source record at all, is what a
    // finished previous run looks like.
    let destination = dir.join("music/Miles Davis");
    std::fs::create_dir_all(&destination).expect("a folder");
    std::fs::write(destination.join("banner.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).expect("written");

    let list = targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
        banner_options(),
    );
    assert!(
        list.is_empty(),
        "the logo is on record and the banner is on disk: nothing left to ask"
    );
}

#[test]
fn store_records_a_written_logo_under_its_own_source_name() {
    let dir = sandbox("store_written");
    let mut layer = Sources::default();
    let target = Target {
        entity: entity(),
        name: "Miles Davis".to_string(),
        mbid: "mbid-1".to_string(),
        destination: dir.join("Miles Davis"),
        albums: Vec::new(),
    };
    store(
        &mut layer,
        &target,
        &Outcome::Written {
            url: "https://assets.fanart.tv/fanart/music/miles-davis/logo.png".to_string(),
            new: true,
        },
    );
    let record = layer
        .get(&entity(), fanarttv::LOGO_SOURCE)
        .expect("a record");
    let Facts::Artist(artist) = &record.facts else {
        panic!("an artist record")
    };
    assert_eq!(
        artist.logo.as_ref().map(|p| p.url.as_str()),
        Some("https://assets.fanart.tv/fanart/music/miles-davis/logo.png")
    );
}

#[test]
fn store_records_nothing_found_too() {
    let mut layer = Sources::default();
    let target = Target {
        entity: entity(),
        name: "Miles Davis".to_string(),
        mbid: "mbid-1".to_string(),
        destination: std::env::temp_dir().join("wherever"),
        albums: Vec::new(),
    };
    store(&mut layer, &target, &Outcome::Nothing);
    let record = layer
        .get(&entity(), fanarttv::LOGO_SOURCE)
        .expect("a record");
    let Facts::Artist(artist) = &record.facts else {
        panic!("an artist record")
    };
    assert!(artist.logo.is_none(), "asked, and there was nothing");
}

#[test]
fn store_writing_a_logo_never_erases_a_portrait_already_on_record() {
    // `--portraits` and `--logos` both end up asking Fanart.tv about the same
    // artist, and both used to be tempted to file the answer under the same
    // source name. `Sources::set` replaces a record whole — so if they ever
    // shared one, whichever pass ran second would silently wipe out what the
    // other had found. `fanarttv::LOGO_SOURCE` exists so that cannot happen;
    // this proves it.
    let mut layer = Sources::default();
    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            portrait: Some(Picture {
                url: "https://assets.fanart.tv/fanart/music/miles-davis/thumb.jpg".to_string(),
            }),
            ..Default::default()
        }),
    });

    let target = Target {
        entity: entity(),
        name: "Miles Davis".to_string(),
        mbid: "mbid-1".to_string(),
        destination: std::env::temp_dir().join("wherever"),
        albums: Vec::new(),
    };
    store(
        &mut layer,
        &target,
        &Outcome::Written {
            url: "https://assets.fanart.tv/fanart/music/miles-davis/logo.png".to_string(),
            new: true,
        },
    );

    let portrait_record = layer.get(&entity(), fanarttv::SOURCE).expect("still there");
    let Facts::Artist(portrait_artist) = &portrait_record.facts else {
        panic!("an artist record")
    };
    assert_eq!(
        portrait_artist.portrait.as_ref().map(|p| p.url.as_str()),
        Some("https://assets.fanart.tv/fanart/music/miles-davis/thumb.jpg"),
        "the portrait fetched earlier must survive a later --logos run"
    );

    let logo_record = layer
        .get(&entity(), fanarttv::LOGO_SOURCE)
        .expect("a record");
    let Facts::Artist(logo_artist) = &logo_record.facts else {
        panic!("an artist record")
    };
    assert_eq!(
        logo_artist.logo.as_ref().map(|p| p.url.as_str()),
        Some("https://assets.fanart.tv/fanart/music/miles-davis/logo.png")
    );
}

#[test]
fn has_logo_reads_the_dedicated_source() {
    let mut layer = Sources::default();
    assert!(!has_logo(&layer, &entity()));

    layer.set(SourceRecord {
        key: "miles davis".to_string(),
        source: fanarttv::LOGO_SOURCE.to_string(),
        source_id: Some("mbid-1".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Artist(ArtistFacts {
            logo: Some(Picture {
                url: "https://fanart.tv/x.png".to_string(),
            }),
            ..Default::default()
        }),
    });
    assert!(has_logo(&layer, &entity()));
}

#[test]
fn an_artist_already_asked_with_no_logo_is_not_asked_forever() {
    let dir = sandbox("empty_answer_is_final");
    let catalog = one_album(&dir);
    let mut layer = held();
    store(
        &mut layer,
        &Target {
            entity: entity(),
            name: "Miles Davis".to_string(),
            mbid: "mbid-1".to_string(),
            destination: dir.join("Miles Davis"),
            albums: Vec::new(),
        },
        &Outcome::Nothing,
    );
    assert!(
        targets(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            &dir,
            false,
            logo_options(),
        )
        .is_empty()
    );
}

#[test]
fn an_identified_label_becomes_a_fanart_label_target() {
    let dir = sandbox("label_target");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("label", "Columbia");
    let catalog = build(
        vec![ScannedFile {
            path: dir
                .join("music/Miles Davis/Kind of Blue/01.flac")
                .to_string_lossy()
                .to_string(),
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
    let entity = EntityRef::of(
        &catalog,
        aede_core::model::EntityKind::Label,
        catalog.labels[0].id,
    )
    .expect("the label is in the catalog");
    let mut layer = Sources::default();
    layer.set(SourceRecord {
        key: entity.key.clone(),
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("label-mbid".to_string()),
        fetched_at: 1,
        confidence: Confidence::Identified,
        facts: Facts::Label(LabelFacts::default()),
    });
    let targets = label_targets(
        &catalog,
        &layer,
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
    );
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].mbid, "label-mbid");
    assert_eq!(
        targets[0].destination,
        store::assets_dir(&dir).join("labels/label-mbid")
    );
}

#[test]
fn an_approximate_label_match_never_becomes_a_fanart_target() {
    let dir = sandbox("label_suggestion");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("label", "Columbia");
    let catalog = build(
        vec![ScannedFile {
            path: dir.join("music/01.flac").to_string_lossy().to_string(),
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
    let entity = EntityRef::of(
        &catalog,
        aede_core::model::EntityKind::Label,
        catalog.labels[0].id,
    )
    .expect("label");
    let mut layer = Sources::default();
    layer.set(SourceRecord {
        key: entity.key,
        source: sources::MUSICBRAINZ.to_string(),
        source_id: Some("candidate-only".to_string()),
        fetched_at: 1,
        confidence: Confidence::matched(99),
        facts: Facts::Label(LabelFacts::default()),
    });

    assert!(
        label_targets(
            &catalog,
            &layer,
            &[],
            &crate::commands::fetch::EVERYTHING,
            &dir,
            false,
        )
        .is_empty()
    );
}

#[test]
fn an_explicit_local_label_id_is_enough_for_a_fanart_target() {
    let dir = sandbox("label_local_id");
    let mut tags = RawTags::default();
    tags.insert("artist", "Miles Davis");
    tags.insert("albumartist", "Miles Davis");
    tags.insert("album", "Kind of Blue");
    tags.insert("title", "So What");
    tags.insert("label", "Columbia");
    tags.insert("musicbrainz_labelid", "local-label-id");
    let catalog = build(
        vec![ScannedFile {
            path: dir.join("music/01.flac").to_string_lossy().to_string(),
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

    let targets = label_targets(
        &catalog,
        &Sources::default(),
        &[],
        &crate::commands::fetch::EVERYTHING,
        &dir,
        false,
    );
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].mbid, "local-label-id");
}

#[test]
fn no_key_means_nothing_is_ever_waiting() {
    let dir = sandbox("waiting_no_key");
    let catalog = one_album(&dir);
    assert_eq!(
        waiting(&catalog, &held(), &dir, None),
        0,
        "fanart.tv is the only source, so no key means nothing this pass can ask"
    );
}

#[test]
fn what_fetch_offers_is_exactly_what_this_pass_would_ask() {
    let dir = sandbox("waiting");
    let catalog = one_album(&dir);
    assert_eq!(waiting(&catalog, &held(), &dir, Some("key")), 1);
}
