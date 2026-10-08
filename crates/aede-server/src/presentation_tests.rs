use super::*;
use crate::test_support::sample_state;
#[test]
fn presentation_preserves_compilations_and_contributor_only_artists() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.credits.push(aede_core::model::Credit {
        artist_id: 0,
        entity_kind: EntityKind::Track,
        entity_id: 0,
        role: "performer".into(),
        attributes: Vec::new(),
        credited_as: None,
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    });
    catalog.releases[0].album_artist_id = None;
    catalog.releases[0].is_compilation = true;
    let album = album_row(&catalog, &catalog.releases[0]).unwrap();
    assert!(album["artist"].is_null());
    assert_eq!(album["compilation"], true);
    assert!(
        album["contributors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r == &reference_of(&catalog, EntityKind::Artist, 0).unwrap())
    );
    let artist = artist_row(
        &catalog,
        &Sources::default(),
        &state.data_dir,
        &catalog.artists[0],
    )
    .unwrap();
    assert_eq!(artist["release_refs"].as_array().unwrap().len(), 1);
    assert!(artist["country"].is_null());
    assert!(artist["biography_credit"].is_null());
}
#[test]
fn presentation_indexes_every_local_tag_without_path_based_identity() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.files[0]
        .tags
        .insert("custom-field".into(), vec!["A rare description".into()]);
    let album = album_row(&catalog, &catalog.releases[0]).unwrap();
    assert!(
        album["search_fields"]
            .as_array()
            .unwrap()
            .contains(&Value::String("A rare description".into()))
    );
    assert!(!album["reference"].as_str().unwrap().starts_with('/'));
}
#[test]
fn presentation_pagination_is_bounded_and_rejects_unknown_parameters() {
    assert!(
        bounds(Ok(Query(Paging {
            offset: Some(0),
            limit: Some(201)
        })))
        .is_err()
    );
    assert!(serde_json::from_str::<Paging>(r#"{"path":"/private"}"#).is_err());
    assert_eq!(
        bounds(Ok(Query(Paging::default()))).unwrap(),
        (0, DEFAULT_LIMIT)
    );
}
