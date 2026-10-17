use super::*;
use crate::model::builder::{ScannedFile, build};
use crate::sources::{ArtistFacts, Confidence, SourceRecord, Sources};
use crate::tags::RawTags;

fn shelf(albums: &[(&str, Option<&str>)]) -> Catalog {
    shelf_for(
        &albums
            .iter()
            .map(|(title, group)| ("Example Artist", *title, *group))
            .collect::<Vec<_>>(),
    )
}

fn shelf_for(albums: &[(&str, &str, Option<&str>)]) -> Catalog {
    let root = std::env::temp_dir().join("aede_missing_identity_fixture");
    let files = albums
        .iter()
        .enumerate()
        .map(|(index, (artist, title, group))| {
            let mut tags = RawTags::default();
            tags.insert("artist", *artist);
            tags.insert("albumartist", *artist);
            tags.insert("album", *title);
            tags.insert("title", "A track");
            if let Some(group) = group {
                tags.insert("musicbrainz_releasegroupid", *group);
            }
            ScannedFile {
                path: root
                    .join(format!("album-{index}/01.flac"))
                    .to_string_lossy()
                    .into_owned(),
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
    build(files, vec![root.to_string_lossy().into_owned()], 1, &[])
}

#[test]
fn owned_group_identifiers_apply_across_artist_shelves_but_titles_do_not() {
    let joint = "11111111-1111-4111-8111-111111111111";
    let own = "22222222-2222-4222-8222-222222222222";
    for (foreign_group, known_group, expected_missing) in [
        (Some(joint), joint, false),
        (None, joint, true),
        (Some(""), "", true),
    ] {
        let mut catalog = shelf_for(&[
            ("First Artist", "Joint Session", foreign_group),
            ("Example Artist", "Another Album", Some(own)),
        ]);
        if foreign_group == Some("") {
            // Blank tag values normally normalize to None. An empty API value
            // must not establish an exact identity either.
            catalog
                .releases
                .iter_mut()
                .find(|release| release.title == "Joint Session")
                .unwrap()
                .release_group_mbid = Some(String::new());
        }
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: "example artist".into(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                discography: vec![KnownRelease {
                    mbid: known_group.into(),
                    title: if known_group.is_empty() {
                        "Unknown identity"
                    } else {
                        "Joint Session"
                    }
                    .into(),
                    first_released: None,
                    primary_type: Some("Album".into()),
                    secondary_types: vec![],
                }],
                ..Default::default()
            }),
        });
        assert_eq!(
            absent(&catalog, &held, &[], false).len(),
            usize::from(expected_missing),
            "{foreign_group:?}, {known_group}"
        );

        // Global identity recognition does not create a shelf for a guest.
        let foreign_artist = catalog
            .artists
            .iter()
            .find(|artist| artist.key == "first artist")
            .unwrap()
            .id;
        for release in &mut catalog.releases {
            release.album_artist_id = Some(foreign_artist);
        }
        assert!(absent(&catalog, &held, &[], false).is_empty());
    }
}

#[test]
fn explicit_group_identity_precedes_title_fallback_without_losing_a_tagged_shelf() {
    let owned = "11111111-1111-4111-8111-111111111111";
    let other = "22222222-2222-4222-8222-222222222222";
    for (albums, known_group, known_title, expected_missing) in [
        (
            vec![("Local spelling", Some(owned))],
            owned,
            "Official spelling",
            false,
        ),
        (
            vec![("Shared Title", Some(owned))],
            other,
            "Shared Title",
            true,
        ),
        (
            vec![("  SHARED  title", None)],
            other,
            "Shared Title",
            false,
        ),
        (
            vec![("Owned title", Some(owned))],
            other,
            "Another title",
            true,
        ),
        (
            vec![("Shared Title", Some(owned)), ("Another title", None)],
            other,
            "Shared Title",
            true,
        ),
        (
            vec![("Owned title", Some(owned)), ("Shared Title", None)],
            other,
            "Shared Title",
            false,
        ),
    ] {
        let catalog = shelf(&albums);
        let mut held = Sources::default();
        held.set(SourceRecord {
            key: "example artist".into(),
            source: sources::MUSICBRAINZ.into(),
            source_id: Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into()),
            fetched_at: 1,
            confidence: Confidence::Identified,
            facts: Facts::Artist(ArtistFacts {
                discography: vec![KnownRelease {
                    mbid: known_group.into(),
                    title: known_title.into(),
                    first_released: Some("2000".into()),
                    primary_type: Some("Album".into()),
                    secondary_types: vec![],
                }],
                discography_fetched_at: Some(1),
                ..Default::default()
            }),
        });
        let before = sources::to_json(&held);
        for everything in [false, true] {
            let missing = absent(&catalog, &held, &[], everything);
            assert_eq!(
                missing.len(),
                usize::from(expected_missing),
                "{albums:?}, {known_group}"
            );
            if expected_missing {
                assert_eq!(missing[0].known.mbid, known_group);
            }
        }
        let artist_id = catalog
            .artists
            .iter()
            .find(|artist| artist.key == "example artist")
            .unwrap()
            .id;
        assert_eq!(
            absent_for(&catalog, &held, &[], artist_id),
            usize::from(expected_missing)
        );
        assert_eq!(sources::to_json(&held), before);
    }
}
