use super::*;
use crate::test_support::sample_state;
#[test]
fn analysis_coverage_partitions_retained_results_and_pages_albums_without_running_jobs() {
    let state = sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    for id in 1..4 {
        let mut f = c.files[0].clone();
        f.id = id;
        f.path = format!("/music/album/{id}.flac");
        c.files.push(f);
        let mut t = c.tracks[0].clone();
        t.id = id;
        t.file_id = id;
        c.tracks.push(t);
        c.releases[0].track_ids.push(id);
    }
    for id in 0..3 {
        let f = &c.files[id];
        c.analyses.push(aede_core::analysis::FileAnalysis {
            path: f.path.clone(),
            source: "flaccompagnon".into(),
            size_bytes: if id == 1 { f.size + 1 } else { f.size },
            modified_unix: f.mtime,
            error: (id == 2).then(|| "decoding failed".into()),
            ..Default::default()
        });
    }
    let result = analysis_coverage::page(&c, 0, 50, None).unwrap();
    for key in ["current", "stale", "failed", "missing"] {
        assert_eq!(result["items"][0][key], 1);
        assert_eq!(result["summary"][key], 1);
    }
    assert_eq!(result["items"][0]["tracks"], 4);
    assert_eq!(
        analysis_coverage::page(&c, 0, 50, Some("not present")).unwrap()["total"],
        0
    );
    assert_eq!(
        analysis_coverage::page(&c, 1, 50, None).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
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
    let album = album_row(
        &catalog,
        &catalog.releases[0],
        &catalog.composers_by_release(),
    )
    .unwrap();
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
    let album = album_row(
        &catalog,
        &catalog.releases[0],
        &catalog.composers_by_release(),
    )
    .unwrap();
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
    let album = album_row(
        &catalog,
        &catalog.releases[0],
        &catalog.composers_by_release(),
    )
    .unwrap();
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
#[test]
fn coverage_sort_precedes_paging_and_preserves_stable_title_ties() {
    let state = sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    let base = c.releases[0].clone();
    c.releases.clear();
    for (id, title, count) in [
        (0, "Z missing", 3),
        (1, "A complete", 0),
        (2, "B missing", 3),
    ] {
        let mut release = base.clone();
        release.id = id;
        release.title = title.into();
        release.key = title.into();
        release.track_ids = vec![0; count];
        c.releases.push(release);
    }
    use analysis_coverage::{Direction, SortColumn};
    let first =
        analysis_coverage::sorted_page(&c, 0, 1, None, SortColumn::Missing, Direction::Desc)
            .unwrap();
    assert_eq!(first["items"][0]["title"], "B missing");
    let next = analysis_coverage::sorted_page(&c, 1, 1, None, SortColumn::Missing, Direction::Desc)
        .unwrap();
    assert_eq!(next["items"][0]["title"], "Z missing");
    let asc = analysis_coverage::sorted_page(&c, 0, 1, None, SortColumn::Missing, Direction::Asc)
        .unwrap();
    assert_eq!(asc["items"][0]["title"], "A complete");
    let title =
        analysis_coverage::sorted_page(&c, 0, 1, None, SortColumn::Title, Direction::Desc).unwrap();
    assert_eq!(title["items"][0]["title"], "Z missing");
    assert_eq!(first["total"], 3);
    assert_eq!(first["summary"], asc["summary"]);
}
#[test]
fn coverage_sort_rejects_unknown_columns_and_directions() {
    assert!(
        serde_json::from_value::<analysis_coverage::Parameters>(json!({"sort":"unknown"})).is_err()
    );
    assert!(
        serde_json::from_value::<analysis_coverage::Parameters>(
            json!({"sort":"missing","direction":"sideways"})
        )
        .is_err()
    );
}

#[test]
fn label_playback_starts_with_oldest_album_and_preserves_disc_positions() {
    let state = sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    c.labels = vec![aede_core::model::Label {
        id: 0,
        key: "test-label".into(),
        name: "Test Label".into(),
        ..Default::default()
    }];
    c.releases[0].label_ids = vec![0];
    c.releases[0].year = Some(2000);
    for id in 1..3 {
        let mut r = c.releases[0].clone();
        r.id = id;
        r.title = format!("Album {id}");
        r.folder = format!("/music/album{id}");
        r.year = Some(1990);
        r.track_ids = vec![id];
        if id == 2 {
            r.label_ids.clear();
        }
        c.releases.push(r);
        let mut t = c.tracks[0].clone();
        t.id = id;
        t.file_id = id;
        t.release_id = Some(id);
        c.tracks.push(t);
        let mut f = c.files[0].clone();
        f.id = id;
        f.path = format!("/music/album{id}/01.flac");
        c.files.push(f);
    }
    let mut second = c.tracks[1].clone();
    second.id = 3;
    second.file_id = 3;
    second.disc_no = Some(2);
    c.tracks.push(second);
    let mut file = c.files[1].clone();
    file.id = 3;
    file.path = "/music/album1/disc2/01.flac".into();
    c.files.push(file);
    c.releases[1].track_ids.push(3);
    let rows = playback_rows(&c, &reference_of(&c, EntityKind::Label, 0).unwrap()).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["album_total"], 2);
    assert_eq!(rows[1]["number"], 2);
    assert_eq!(rows[1]["album_total"], 2);
    assert_eq!(
        rows[0]["release"],
        reference_of(&c, EntityKind::Release, 1).unwrap()
    );
    assert_eq!(rows[0]["number"], 1);
    assert_eq!(
        rows[2]["release"],
        reference_of(&c, EntityKind::Release, 0).unwrap()
    );
}

#[test]
fn track_pages_expose_lyrics_presence_without_lyrics_payloads() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    let page = track_pages::listing(&catalog, None, 0, 50, None).unwrap();
    assert_eq!(page["items"][0]["lyrics_available"], false);
    catalog.files[0]
        .tags
        .insert("lyrics".into(), vec!["original control words".into()]);
    let page = track_pages::listing(&catalog, None, 0, 50, None).unwrap();
    assert_eq!(page["items"][0]["lyrics_available"], true);
    assert!(page["items"][0].get("lyrics").is_none());
}

#[test]
fn coverage_integrity_distinguishes_md5_crc_absent_unverified_and_unsupported() {
    use aede_core::analysis::FileAnalysis;
    let state = crate::test_support::sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    let file = &mut c.files[0];
    file.properties.codec = "flac".into();
    let report = FileAnalysis {
        source: "flaccompagnon".into(),
        path: file.path.clone(),
        size_bytes: file.size,
        modified_unix: file.mtime,
        md5_state: Some("Match".into()),
        ..Default::default()
    };
    c.analyses = vec![report];
    assert_eq!(
        analysis_coverage::page(&c, 0, 50, None).unwrap()["items"][0]["integrity"]["ok"],
        1
    );
    c.analyses[0].md5_state = Some("Mismatch".into());
    assert_eq!(
        analysis_coverage::page(&c, 0, 50, None).unwrap()["items"][0]["integrity"]["ko"],
        1
    );
    c.analyses[0].md5_state = Some("NoSignature".into());
    assert_eq!(
        analysis_coverage::page(&c, 0, 50, None).unwrap()["items"][0]["integrity"]["no_hash"],
        1
    );
    c.analyses.clear();
    assert_eq!(
        analysis_coverage::page(&c, 0, 50, None).unwrap()["items"][0]["integrity"]["unchecked"],
        1
    );
    c.files[0].properties.codec = "mp3".into();
    assert_eq!(
        analysis_coverage::page(&c, 0, 50, None).unwrap()["items"][0]["integrity"]["unsupported"],
        1
    );
}

#[test]
fn presentation_distinguishes_present_and_missing_artist_logos() {
    let state = sample_state();
    let mut catalog = state.catalog.blocking_read().clone().unwrap();
    let root = std::env::temp_dir().join(format!("aede-logo-presence-{}", std::process::id()));
    let folder = root.join("Artist");
    std::fs::create_dir_all(folder.join("Album")).unwrap();
    catalog.roots = vec![root.to_str().unwrap().into()];
    catalog.releases[0].folder = folder.join("Album").to_str().unwrap().into();
    let sources = Sources::default();
    let absent = artist_row(&catalog, &sources, &state.data_dir, &catalog.artists[0]).unwrap();
    assert_eq!(absent["media"]["logo"], false);
    let path = folder.join(aede_core::coverart::image_name(Kind::Logo, "png", 0, 1));
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
    let present = artist_row(&catalog, &sources, &state.data_dir, &catalog.artists[0]).unwrap();
    assert_eq!(present["media"]["logo"], true);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn album_projection_distinguishes_composers_from_performers_and_deduplicates_credits() {
    let state = sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    c.credits.clear();
    let credit = aede_core::model::Credit {
        artist_id: 0,
        entity_kind: EntityKind::Track,
        entity_id: 0,
        role: "performer".into(),
        attributes: vec![],
        credited_as: None,
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    };
    c.credits.push(credit.clone());
    assert_eq!(
        album_row(&c, &c.releases[0], &c.composers_by_release()).unwrap()["composers"],
        json!([])
    );
    let mut composer = credit;
    composer.role = "composer".into();
    c.credits.extend([composer.clone(), composer]);
    let expected = json!([{"reference":reference_of(&c, EntityKind::Artist, 0).unwrap()}]);
    assert!(
        album_row(&c, &c.releases[0], &c.composers_by_release()).unwrap()["search_fields"]
            .as_array()
            .unwrap()
            .contains(&json!(c.artists[0].name))
    );
    assert_eq!(
        album_row(&c, &c.releases[0], &c.composers_by_release()).unwrap()["composers"],
        expected
    );
}

#[test]
fn composer_index_follows_recording_and_work_placements_without_performer_inference() {
    let state = sample_state();
    let mut c = state.catalog.blocking_read().clone().unwrap();
    let recording = c.tracks[0].recording_id;
    c.works = vec![aede_core::model::Work {
        id: 0,
        mbid: "55555555-5555-5555-5555-555555555555".into(),
        title: "Composition".into(),
        recording_ids: vec![recording],
        ..Default::default()
    }];
    c.recordings[recording as usize].work_ids.push(0);
    c.credits.clear();
    let mut credit = aede_core::model::Credit {
        artist_id: 0,
        entity_kind: EntityKind::Work,
        entity_id: 0,
        role: "composer".into(),
        attributes: vec![],
        credited_as: None,
        began: None,
        ended: None,
        order: None,
        source: "tags".into(),
        source_id: None,
    };
    for kind in [EntityKind::Work, EntityKind::Recording, EntityKind::Release] {
        credit.entity_kind = kind;
        credit.entity_id = if kind == EntityKind::Recording {
            recording
        } else {
            0
        };
        c.credits = vec![credit.clone()];
        assert_eq!(c.composers_by_release()[&0], BTreeSet::from([0]));
    }
    credit.role = "performer".into();
    c.credits = vec![credit];
    assert!(c.composers_by_release().is_empty());
}
