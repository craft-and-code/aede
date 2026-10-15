use super::*;
#[test]
fn principal_artist_tags_select_owned_tracks_without_guest_or_other_owner_leakage() {
    let state = crate::test_support::sample_state();
    let guard = state.catalog.try_read().unwrap();
    let catalog = guard.as_ref().unwrap();
    let track = EntityRef::of(catalog, EntityKind::Track, 0).unwrap();
    let release = catalog
        .release(catalog.track(0).unwrap().release_id.unwrap())
        .unwrap();
    let artist = EntityRef::of(
        catalog,
        EntityKind::Artist,
        release.album_artist_id.unwrap(),
    )
    .unwrap();
    let mut data = UserData::default();
    data.entry("one", &artist, 1)
        .tags
        .insert("relaxation".into());
    let calm = domain(&rules(&data, "one").unwrap()[0]);
    assert_eq!(
        matches(&evidence(catalog, &data, "one").unwrap(), &calm).len(),
        1
    );
    assert!(matches(&evidence(catalog, &data, "two").unwrap(), &calm).is_empty());
    data.entry("one", &track, 2).tags.insert("calme".into());
    assert_eq!(
        matches(&evidence(catalog, &data, "one").unwrap(), &calm).len(),
        1
    );
    assert_eq!(
        matches(&evidence(catalog, &data, "one").unwrap(), &calm)[0]
            .1
            .tag
            .as_deref(),
        Some("calme")
    );
}

#[test]
fn mood_http_rules_remain_owner_private_and_select_normalized_artist_tags() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{encoded, start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let artist = {
            let guard = fixture.0.catalog.read().await;
            let catalog = guard.as_ref().unwrap();
            let release = catalog
                .release(catalog.track(0).unwrap().release_id.unwrap())
                .unwrap();
            EntityRef::of(
                catalog,
                EntityKind::Artist,
                release.album_artist_id.unwrap(),
            )
            .unwrap()
            .to_token()
        };
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let auditor = login(address, "auditor");
        let path = "/api/me/v1/moods";
        assert_eq!(http(address, "GET", path, None, "").0, 401);
        let rule = r#"{"id":"winter","name":"Winter","tags":["Concentration"],"genres":[]}"#;
        assert_eq!(http(address, "PUT", path, Some(&auditor), rule).0, 403);
        assert_eq!(http(address, "PUT", path, Some(&alice), rule).0, 200);
        let annotation = format!("/api/me/v1/annotation?ref={}", encoded(&artist));
        assert_eq!(
            http(
                address,
                "PUT",
                &annotation,
                Some(&alice),
                r#"{"tags":["concentration"]}"#
            )
            .0,
            200
        );
        let (status, page, _) = http(
            address,
            "GET",
            "/api/me/v1/moods?mood=winter&limit=1",
            Some(&alice),
            "",
        );
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["total"], 1);
        assert_eq!(page["items"][0]["reason"]["tag"], "concentration");
        assert_eq!(page["items"][0]["reason"]["scope"], "artist");
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/moods?mood=winter",
                Some(&bob),
                ""
            )
            .0,
            400
        );
        let bytes = std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap();
        for invalid in [
            r#"{"id":"winter","name":"Winter","tags":[],"genres":[]}"#,
            r#"{"id":"winter","name":"Winter","tags":["x"],"genres":[],"ignored":true}"#,
        ] {
            assert_eq!(http(address, "PUT", path, Some(&alice), invalid).0, 400);
        }
        assert_eq!(
            std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap(),
            bytes
        );
        server.abort();
    });
}

#[test]
fn explicit_track_mood_overrides_inherited_artist_atmosphere() {
    let state = crate::test_support::sample_state();
    let guard = state.catalog.try_read().unwrap();
    let catalog = guard.as_ref().unwrap();
    let track = EntityRef::of(catalog, EntityKind::Track, 0).unwrap();
    let release = catalog
        .release(catalog.track(0).unwrap().release_id.unwrap())
        .unwrap();
    let artist = EntityRef::of(
        catalog,
        EntityKind::Artist,
        release.album_artist_id.unwrap(),
    )
    .unwrap();
    let mut data = UserData::default();
    data.entry("one", &artist, 1).tags.insert("energy".into());
    data.entry("one", &track, 2).tags.insert("calm".into());
    let rows = evidence(catalog, &data, "one").unwrap();
    assert_eq!(matches(&rows, &aede_moods::presets()[0]).len(), 1);
    assert!(matches(&rows, &aede_moods::presets()[1]).is_empty());
}

#[test]
fn mood_results_rank_specific_evidence_before_stable_chronological_ties() {
    let rule = aede_moods::presets().remove(0);
    let rows: Vec<_> = [
        TagScope::Genre,
        TagScope::Artist,
        TagScope::Track,
        TagScope::Album,
        TagScope::Track,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, scope)| Evidence {
        file_id: index as Id,
        reference: EntityRef::new(EntityKind::Track, format!("file-{index}")),
        tags: if scope == TagScope::Genre {
            vec![]
        } else {
            vec!["calm".into()]
        },
        genres: vec!["ambient".into()],
        scope,
    })
    .collect();
    let selected = matches(&rows, &rule);
    assert_eq!(
        selected
            .iter()
            .map(|row| row.0.to_token())
            .collect::<Vec<_>>(),
        [
            "track:file-2",
            "track:file-4",
            "track:file-3",
            "track:file-1",
            "track:file-0"
        ]
    );
    assert_eq!(selected[0].1.tag.as_deref(), Some("calm"));
    assert_eq!(selected[4].1.genre.as_deref(), Some("ambient"));
}

#[test]
fn adding_a_mood_cannot_overflow_preferences_or_corrupt_another_owners_store() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let owner = fixture.accounts().find("alice").unwrap().id.clone();
        let path = user::user_path(&fixture.0.data_dir);
        let (address, server) = start_server(fixture.0.clone()).await;
        let token = login(address, "alice");
        for (count, own_rows) in [(8192, false), (64, true)] {
            let data = UserData {
                client_preferences: (0..count)
                    .map(|index| user::ClientPreference {
                        owner: if own_rows {
                            owner.clone()
                        } else {
                            format!("account:{}", index / 64)
                        },
                        key: format!("phemios.setting-{}", index % 64),
                        value: "true".into(),
                    })
                    .collect(),
                ..Default::default()
            };
            user::save(&data, &path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let (status, response, _) = http(
                address,
                "PUT",
                "/api/me/v1/moods",
                Some(&token),
                r#"{"id":"winter","name":"Winter","tags":["calm"],"genres":[]}"#,
            );
            assert_eq!(status, 400, "{response}");
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(
                user::load(&path).unwrap().unwrap().client_preferences.len(),
                count
            );
        }
        server.abort();
    });
}
