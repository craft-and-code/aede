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

#[test]
fn stale_cover_metadata_does_not_advertise_an_available_image() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.releases[0].folder = "/definitely-missing-phemios-library/album".into();
    catalog.releases[0].cover_path =
        Some("/definitely-missing-phemios-library/album/cover.jpg".into());
    let album = album_row(&catalog, &catalog.releases[0]).unwrap();
    assert_eq!(album["cover"], false);
}

#[test]
fn playback_selection_keeps_album_track_positions_and_rejects_wrong_reference_kinds() {
    let state = sample_state();
    let c = state.catalog.blocking_read();
    let c = c.as_ref().unwrap();
    let token = reference_of(c, EntityKind::Release, 0).unwrap();
    let rows = playback_rows(c, &token).unwrap();
    assert_eq!(rows.len(), c.releases[0].track_ids.len());
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["number"], index + 1);
        assert_eq!(row["album_total"], rows.len());
        assert_eq!(row["release"], token);
    }
    let single = playback_rows(c, &reference_of(c, EntityKind::Track, 0).unwrap()).unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single[0]["number"], 1);
    assert_eq!(single[0]["release"], token);
    let mut detached = c.clone();
    detached.tracks[0].release_id = None;
    let single = playback_rows(
        &detached,
        &reference_of(&detached, EntityKind::Track, 0).unwrap(),
    )
    .unwrap();
    assert!(single[0]["release"].is_null());
    assert_eq!(single[0]["album_total"], 1);
    assert!(playback_rows(c, &reference_of(c, EntityKind::Recording, 0).unwrap()).is_err());
}

#[test]
fn album_track_pages_preserve_four_discs_without_loading_analysis_payloads() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    catalog.tracks.clear();
    catalog.files.clear();
    catalog.releases[0].track_ids.clear();
    for id in 0..85 {
        let disc = id / 22 + 1;
        let mut track = aede_core::model::Track {
            id,
            file_id: id,
            release_id: Some(0),
            title: format!("Theme {}", id % 22 + 1),
            disc_no: Some(disc),
            track_no: Some(id % 22 + 1),
            ..Default::default()
        };
        if id == 84 {
            track.disc_no = None;
            track.track_no = None;
        }
        catalog.tracks.push(track);
        catalog.files.push(aede_core::model::AudioFile {
            id,
            path: format!("/music/edition/disc-{disc}/{id}.flac"),
            ..Default::default()
        });
        catalog.releases[0].track_ids.push(id);
    }
    let reference = reference_of(&catalog, EntityKind::Release, 0).unwrap();
    let first = track_pages::page(&catalog, &reference, 0, 40).unwrap();
    let second = track_pages::page(&catalog, &reference, 40, 200).unwrap();
    assert_eq!(first["total"], 85);
    assert_eq!(first["items"].as_array().unwrap().len(), 40);
    assert_eq!(second["items"].as_array().unwrap().len(), 45);
    assert_eq!(first["items"][22]["disc_no"], 2);
    assert_eq!(first["items"][22]["track_no"], 1);
    assert_eq!(first["items"][22]["album_position"], 23);
    assert_ne!(
        first["items"][0]["reference"],
        first["items"][22]["reference"]
    );
    assert!(first["items"][0].get("analyses").is_none());
    assert_eq!(first["items"][0]["analysis_available"], false);
    assert!(second["items"][44]["disc_no"].is_null());
    assert!(track_pages::page(&catalog, "artist:fictional", 0, 40).is_err());
    assert!(
        track_pages::page(&catalog, &reference, 85, 200).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn status_advertises_dynamic_collection_and_album_page_contracts() {
    crate::test_support::test_runtime().block_on(async {
        let response = crate::catalog::status(State(sample_state())).await;
        let value = serde_json::to_value(response.0).unwrap();
        assert_eq!(value["api_version"], 1);
        assert!(
            value["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "dynamic_browse_collections")
        );
        assert!(
            value["capabilities"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "album_track_pages")
        );
    });
}

#[test]
fn complete_track_pages_search_tags_and_preserve_albumless_rows() {
    let state = sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    c.files[0]
        .tags
        .insert("mood".into(), vec!["Électricité".into()]);
    c.tracks[0].release_id = None;
    let page = track_pages::listing(&c, None, 0, 50, Some("electricite")).unwrap();
    assert_eq!(page["total"], 1);
    assert!(page["items"][0]["release"].is_null());
    assert!(page["items"][0]["album_position"].is_null());
    assert!(page["items"][0].get("analyses").is_none());
    c.credits.push(aede_core::model::Credit {
        artist_id: 0,
        entity_kind: EntityKind::Track,
        entity_id: 0,
        role: "theremin".into(),
        source: "tags".into(),
        attributes: Vec::new(),
        credited_as: None,
        began: None,
        ended: None,
        order: None,
        source_id: None,
    });
    assert_eq!(
        track_pages::listing(&c, None, 0, 50, Some("theremin")).unwrap()["total"],
        1
    );
    c.credits[0].entity_kind = EntityKind::Release;
    assert_eq!(
        track_pages::listing(&c, None, 0, 50, Some("theremin")).unwrap()["total"],
        0
    );
    let empty = track_pages::listing(&c, None, 0, 50, Some("missing-metadata")).unwrap();
    assert_eq!(empty["total"], 0);
}
