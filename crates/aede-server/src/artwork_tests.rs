use super::*;
use crate::test_support::sample_state;
#[test]
fn artwork_selection_rejects_paths_and_wrong_entity_kinds() {
    let state = sample_state();
    let guard = state.catalog.blocking_read();
    let catalog = guard.as_ref().unwrap();
    for (reference, kind) in [
        ("/private/cover.jpg".into(), "cover"),
        (reference(catalog, EntityKind::Artist, 0).unwrap(), "cover"),
        (
            reference(catalog, EntityKind::Release, 0).unwrap(),
            "../background",
        ),
    ] {
        assert!(
            select(
                catalog,
                &state.data_dir,
                &Parameters {
                    reference,
                    kind: kind.into(),
                    size: None
                }
            )
            .is_err()
        );
    }
}
#[test]
fn artist_assets_never_escape_through_a_tagged_identifier_or_watched_root() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.artists[0].mbid = Some("../../private".into());
    catalog.releases[0].folder = "/music/album".into();
    catalog.roots = vec!["/music".into()];
    assert!(artist_folder(&catalog, 0, &state.data_dir).is_none());
    catalog.artists[0].mbid = Some("12345678-1234-1234-1234-123456789abc".into());
    assert_eq!(
        artist_folder(&catalog, 0, &state.data_dir),
        Some(
            state
                .data_dir
                .join("assets/artists/12345678-1234-1234-1234-123456789abc")
        )
    );
}

#[test]
fn artist_logo_uses_the_same_confined_asset_and_presence_contract_as_portraits() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    let root = std::env::temp_dir().join(format!("aede-logo-presentation-{}", std::process::id()));
    let artist_folder = root.join("Artist");
    std::fs::create_dir_all(artist_folder.join("Album")).unwrap();
    catalog.roots = vec![root.to_str().unwrap().into()];
    catalog.releases[0].folder = artist_folder.join("Album").to_str().unwrap().into();
    let logo = artist_folder.join(coverart::image_name(Kind::Logo, "png", 0, 1));
    std::fs::write(&logo, b"\x89PNG\r\n\x1a\n").unwrap();
    let params = Parameters {
        reference: reference(&catalog, EntityKind::Artist, 0).unwrap(),
        kind: "logo".into(),
        size: Some(512),
    };
    let source = select(&catalog, &state.data_dir, &params).expect("catalogue artist logo");
    assert_eq!(source.path, logo);
    std::fs::remove_file(logo).unwrap();
    assert!(select(&catalog, &state.data_dir, &params).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
