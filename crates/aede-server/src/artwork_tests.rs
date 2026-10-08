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
