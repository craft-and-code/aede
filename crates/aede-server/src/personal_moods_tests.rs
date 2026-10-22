use super::*;

#[test]
fn measured_recipe_ids_are_refused_by_mood_put_without_changing_any_owner() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let route = "/api/me/v1/moods";
        for (token, id) in [(&alice, "alice-rule"), (&bob, "bob-rule")] {
            let body = serde_json::json!({"id":id,"name":id,"tags":["calm"],"genres":[]});
            assert_eq!(
                http(address, "PUT", route, Some(token), &body.to_string()).0,
                200
            );
        }
        let alice_before = http(address, "GET", route, Some(&alice), "").1;
        let bob_before = http(address, "GET", route, Some(&bob), "").1;
        let path = user::user_path(&fixture.0.data_dir);
        let before = std::fs::read(&path).unwrap();
        let mut results = Vec::new();
        let mut stored = Vec::new();
        for recipe in aede_moods::audio_recipes() {
            let body =
                serde_json::json!({"id":recipe.id,"name":"Custom","tags":["calm"],"genres":[]});
            let (status, response, _) =
                http(address, "PUT", route, Some(&alice), &body.to_string());
            results.push((status, response["error"]["code"].clone()));
            stored.push(std::fs::read(&path).unwrap());
        }
        assert_eq!(results, vec![(400, serde_json::json!("invalid_query")); 4]);
        for actual in stored {
            assert_eq!(actual, before);
        }
        assert_eq!(
            http(address, "GET", route, Some(&alice), "").1,
            alice_before
        );
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);

        for id in ["winter", "calm", "@aede/audio/custom"] {
            let body =
                serde_json::json!({"id":id,"name":"My rule","tags":["Concentration"],"genres":[]});
            let (status, response, _) =
                http(address, "PUT", route, Some(&alice), &body.to_string());
            assert_eq!(status, 200, "{response}");
            assert_eq!(response["tags"], serde_json::json!(["concentration"]));
            let (status, page, _) = http(
                address,
                "GET",
                &format!("{route}?mood={id}"),
                Some(&alice),
                "",
            );
            assert_eq!(status, 200, "{page}");
            assert_eq!(
                page["moods"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["rule"]["id"] == id)
                    .unwrap()["rule"]["name"],
                "My rule"
            );
        }
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);
        server.abort();
    });
}

#[test]
fn persisted_reserved_moods_are_reported_without_replacing_recipes_or_losing_preferences() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let accounts = fixture.accounts();
        let alice_id = accounts.find("alice").unwrap().id.clone();
        let bob_id = accounts.find("bob").unwrap().id.clone();
        let reserved: Vec<_> = aede_moods::audio_recipes().into_iter().map(|row| row.id).collect();
        let mut custom: Vec<_> = reserved.iter().map(|id| serde_json::json!({"id":id,"name":"Old custom rule","tags":["legacy"],"genres":[]})).collect();
        custom.push(serde_json::json!({"id":"winter","name":"Winter","tags":["calm"],"genres":[]}));
        let data = {
            let guard = fixture.0.catalog.read().await;
            let catalog = guard.as_ref().unwrap();
            let track = EntityRef::of(catalog, EntityKind::Track, 0).unwrap();
            let mut data = UserData::default();
            data.entry(&alice_id, &track, 1).tags.insert("legacy".into());
            data.client_preferences = vec![
                user::ClientPreference {owner:alice_id.clone(),key:"phemios.moods".into(),value:serde_json::to_string(&custom).unwrap()},
                user::ClientPreference {owner:alice_id.clone(),key:"phemios.retained".into(),value:"{\"language\":\"en\"}".into()},
                user::ClientPreference {owner:bob_id,key:"phemios.moods".into(),value:r#"[{"id":"bob-rule","name":"Bob rule","tags":["energy"],"genres":[]}]"#.into()},
            ];
            data
        };
        let path = user::user_path(&fixture.0.data_dir);
        user::save(&data, &path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let preferences = "/api/me/v1/preferences";
        let raw_before = http(address, "GET", preferences, Some(&alice), "").1;
        let bob_before = http(address, "GET", preferences, Some(&bob), "").1;
        let mut results = Vec::new();
        let mut conflicts = Vec::new();
        for recipe in aede_moods::audio_recipes() {
            let (status, page, _) = http(address, "GET", &format!("/api/me/v1/moods?audio=true&mood={}", recipe.id), Some(&alice), "");
            assert_eq!(status, 200, "{page}");
            let selected = page["moods"].as_array().unwrap().iter().filter(|row| row["rule"]["id"] == recipe.id).collect::<Vec<_>>();
            assert_eq!(selected.len(), 1);
            results.push((page["total"].clone(), selected[0]["rule"]["acoustic"].clone()));
            conflicts.push(page["reserved_rule_ids"].clone());
        }
        assert_eq!(results, aede_moods::audio_recipes().into_iter().map(|row| (serde_json::json!(0), serde_json::json!(row.kind))).collect::<Vec<_>>());
        assert_eq!(conflicts, vec![serde_json::json!(reserved); 4]);
        let (status, legacy, _) = http(address, "GET", "/api/me/v1/moods", Some(&alice), "");
        assert_eq!(status, 200, "{legacy}");
        assert_eq!(legacy["reserved_rule_ids"], serde_json::json!(reserved));
        assert!(legacy["moods"].as_array().unwrap().iter().all(|row| !reserved.iter().any(|id| row["rule"]["id"] == *id)));
        assert!(rules(&data, &alice_id).unwrap().iter().all(|row| !reserved.contains(&row.id.as_str())));
        let guard = fixture.0.catalog.read().await;
        assert_eq!(evidence(guard.as_ref().unwrap(), &data, &alice_id).unwrap()[0].scope, TagScope::Genre);
        drop(guard);
        let (status, other, _) = http(address, "GET", "/api/me/v1/moods?audio=true", Some(&bob), "");
        assert_eq!(status, 200, "{other}");
        assert!(other.get("reserved_rule_ids").is_none());
        assert_eq!(http(address, "GET", preferences, Some(&alice), "").1, raw_before);
        assert_eq!(http(address, "GET", preferences, Some(&bob), "").1, bob_before);
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let body = r#"{"id":"autumn","name":"Autumn","tags":["focus"],"genres":[]}"#;
        assert_eq!(http(address, "PUT", "/api/me/v1/moods", Some(&alice), body).0, 200);
        let raw_after = http(address, "GET", preferences, Some(&alice), "").1;
        assert_eq!(&raw_after["values"]["phemios.moods"].as_array().unwrap()[..custom.len()], custom.as_slice());
        assert_eq!(raw_after["values"]["phemios.retained"], raw_before["values"]["phemios.retained"]);
        assert_eq!(http(address, "GET", preferences, Some(&bob), "").1, bob_before);
        assert_eq!(http(address, "PATCH", preferences, Some(&alice), r#"{"values":{"phemios.moods":[{"id":"winter","name":"Winter","tags":["calm"],"genres":[]}]}}"#).0, 200);
        let (status, repaired, _) = http(address, "GET", "/api/me/v1/moods?audio=true", Some(&alice), "");
        assert_eq!(status, 200, "{repaired}");
        assert!(repaired.get("reserved_rule_ids").is_none());
        assert_eq!(http(address, "GET", preferences, Some(&bob), "").1, bob_before);
        server.abort();
    });
}

#[test]
fn mood_put_rejects_unknown_query_without_mutating_any_owner() {
    assert_mood_put_query_is_refused("ignored=true");
}

#[test]
fn mood_put_rejects_owner_query_without_mutating_any_owner() {
    assert_mood_put_query_is_refused("owner=bob");
}

fn assert_mood_put_query_is_refused(query: &str) {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let route = "/api/me/v1/moods";
        for (token, body) in [
            (
                &alice,
                r#"{"id":"alice-rule","name":"Alice rule","tags":["calm"],"genres":[]}"#,
            ),
            (
                &bob,
                r#"{"id":"bob-rule","name":"Bob rule","tags":["energy"],"genres":[]}"#,
            ),
        ] {
            assert_eq!(http(address, "PUT", route, Some(token), body).0, 200);
        }
        let alice_before = http(address, "GET", route, Some(&alice), "").1;
        let bob_before = http(address, "GET", route, Some(&bob), "").1;
        let path = user::user_path(&fixture.0.data_dir);
        let before = std::fs::read(&path).unwrap();
        let body = r#"{"id":"probe","name":"Probe","tags":["concentration"],"genres":[]}"#;
        let (status, response, _) = http(
            address,
            "PUT",
            &format!("{route}?{query}"),
            Some(&alice),
            body,
        );
        assert_eq!(status, 400, "{response}");
        assert_eq!(response["error"]["code"], "invalid_query");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            http(address, "GET", route, Some(&alice), "").1,
            alice_before
        );
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);

        assert_eq!(http(address, "PUT", route, Some(&alice), body).0, 200);
        let (status, own, _) = http(
            address,
            "GET",
            &format!("{route}?mood=probe"),
            Some(&alice),
            "",
        );
        assert_eq!(status, 200, "{own}");
        assert!(
            own["moods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["rule"]["id"] == "probe")
        );
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);
        server.abort();
    });
}

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
