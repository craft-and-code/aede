use super::*;

#[test]
fn preference_patches_cannot_replace_measured_recipes_or_change_any_owner_on_refusal() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let route = "/api/me/v1/preferences";
        for (token, id) in [(&alice, "alice-rule"), (&bob, "bob-rule")] {
            let body = serde_json::json!({"values":{"phemios.retained":{"language":"en"},"phemios.moods":[{"id":id,"name":id,"tags":["calm"],"genres":[]}]}});
            assert_eq!(http(address, "PATCH", route, Some(token), &body.to_string()).0, 200);
        }
        let alice_before = http(address, "GET", route, Some(&alice), "").1;
        let bob_before = http(address, "GET", route, Some(&bob), "").1;
        let path = user::user_path(&fixture.0.data_dir);
        let before = std::fs::read(&path).unwrap();
        let mut results = Vec::new();
        let mut stored = Vec::new();
        for recipe in aede_moods::audio_recipes() {
            for rule in [serde_json::json!({"id":recipe.id,"name":"Custom","tags":["calm"],"genres":[]}), serde_json::json!({"id":recipe.id})] {
                let body = serde_json::json!({"values":{"phemios.retained":false,"phemios.moods":[rule]}});
                let (status, response, _) = http(address, "PATCH", route, Some(&alice), &body.to_string());
                results.push((status, response["error"]["code"].clone()));
                stored.push(std::fs::read(&path).unwrap());
            }
        }
        assert_eq!(results, vec![(400, serde_json::json!("invalid_query")); 8]);
        for actual in stored {
            assert_eq!(actual, before);
        }
        assert_eq!(http(address, "GET", route, Some(&alice), "").1, alice_before);
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);

        let body = serde_json::json!({"values":{"phemios.moods":[{"id":"winter","name":"Winter","tags":["calm"],"genres":[]},{"id":"calm","name":"My calm","tags":["focus"],"genres":[]}],"phemios.list":[1,null,"@aede/audio/slow"]}});
        let (status, response, _) = http(address, "PATCH", route, Some(&alice), &body.to_string());
        assert_eq!(status, 200, "{response}");
        assert_eq!(response["values"]["phemios.moods"], body["values"]["phemios.moods"]);
        assert_eq!(response["values"]["phemios.retained"], alice_before["values"]["phemios.retained"]);
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);
        let (status, custom, _) = http(address, "GET", "/api/me/v1/moods?mood=winter", Some(&alice), "");
        assert_eq!(status, 200, "{custom}");
        assert!(custom["moods"].as_array().unwrap().iter().any(|row| row["rule"]["id"] == "calm" && row["rule"]["name"] == "My calm"));
        server.abort();
    });
}

#[test]
fn mood_preference_json_without_reserved_rule_ids_remains_untyped() {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        for value in [
            serde_json::json!(null),
            serde_json::json!({"legacy":"arbitrary","metadata":["@aede/audio/slow"]}),
            serde_json::json!([{"id":"custom","future":true}]),
        ] {
            let body = serde_json::json!({"values":{"phemios.moods":value}});
            let (status, response, _) = http(
                address,
                "PATCH",
                "/api/me/v1/preferences",
                Some(&alice),
                &body.to_string(),
            );
            assert_eq!(status, 200, "{response}");
            assert_eq!(response["values"]["phemios.moods"], value);
        }
        server.abort();
    });
}

#[test]
fn duplicate_preference_keys_are_refused_without_mutating_any_owner() {
    assert_ambiguous_preferences_are_refused(
        r#"{"values":{"phemios.setting":true,"phemios.setting":false}}"#,
    );
}

#[test]
fn escaped_equivalent_preference_keys_are_refused_without_mutating_any_owner() {
    assert_ambiguous_preferences_are_refused(
        r#"{"values":{"phemios.setting":true,"phemios.\u0073etting":false}}"#,
    );
}

fn assert_ambiguous_preferences_are_refused(body: &str) {
    use crate::accounts_test_support::{Fixture, http, login};
    use crate::test_support::{start_server, test_runtime};
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = start_server(fixture.0.clone()).await;
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let route = "/api/me/v1/preferences";
        for (token, body) in [
            (
                &alice,
                r#"{"values":{"phemios.setting":true,"phemios.retained":{"language":"en"}}}"#,
            ),
            (&bob, r#"{"values":{"phemios.setting":"other owner"}}"#),
        ] {
            assert_eq!(http(address, "PATCH", route, Some(token), body).0, 200);
        }
        let alice_before = http(address, "GET", route, Some(&alice), "").1;
        let bob_before = http(address, "GET", route, Some(&bob), "").1;
        let path = user::user_path(&fixture.0.data_dir);
        let before = std::fs::read(&path).unwrap();
        let (status, response, _) = http(address, "PATCH", route, Some(&alice), body);
        assert_eq!(status, 400, "{response}");
        assert_eq!(response["error"]["code"], "invalid_body");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            http(address, "GET", route, Some(&alice), "").1,
            alice_before
        );
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);

        let (status, own, _) = http(
            address,
            "PATCH",
            route,
            Some(&alice),
            r#"{"values":{"phemios.setting":false,"phemios.list":[1,null,"value"]}}"#,
        );
        assert_eq!(status, 200, "{own}");
        assert_eq!(own["values"]["phemios.setting"], false);
        assert_eq!(
            own["values"]["phemios.list"],
            serde_json::json!([1, null, "value"])
        );
        assert_eq!(
            own["values"]["phemios.retained"],
            serde_json::json!({"language":"en"})
        );
        assert_eq!(http(address, "GET", route, Some(&bob), "").1, bob_before);
        server.abort();
    });
}

#[test]
fn preferences_patch_is_owner_private_and_preserves_other_fields_without_catalog() {
    let mut state = crate::test_support::sample_state();
    state.data_dir = std::env::temp_dir().join(format!(
        "aede-preferences-{}-{}",
        std::process::id(),
        clock::now_seconds()
    ));
    let owner = |id: &str| PersonalOwner {
        id: id.into(),
        principal: None,
    };
    let patch = |key: &str, value: serde_json::Value| {
        Some(Patch {
            values: BTreeMap::from([(key.into(), value)]),
        })
    };
    locked(
        &state,
        &owner("one"),
        patch("phemios.duration", serde_json::json!(true)),
    )
    .unwrap();
    locked(
        &state,
        &owner("one"),
        patch("phemios.appearance", serde_json::json!({"language":"en"})),
    )
    .unwrap();
    locked(
        &state,
        &owner("two"),
        patch("phemios.duration", serde_json::json!(false)),
    )
    .unwrap();
    let a = locked(&state, &owner("one"), None).unwrap();
    assert_eq!(a.values["phemios.duration"], true);
    assert_eq!(a.values.len(), 2);
    assert_eq!(locked(&state, &owner("two"), None).unwrap().values.len(), 1);
    std::fs::write(user::user_path(&state.data_dir), "corrupt").unwrap();
    assert!(
        locked(
            &state,
            &owner("one"),
            patch("phemios.duration", serde_json::json!(false))
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(user::user_path(&state.data_dir)).unwrap(),
        "corrupt"
    );
    std::fs::remove_dir_all(state.data_dir).unwrap();
}
#[test]
fn preference_http_writes_are_private_role_checked_and_patch_fields_independently() {
    use crate::accounts_test_support::{Fixture, http, login};
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
        let one = login(address, "alice");
        let two = login(address, "bob");
        let auditor = login(address, "auditor");
        let route = "/api/me/v1/preferences";
        assert_eq!(
            http(
                address,
                "PATCH",
                route,
                Some(&one),
                r#"{"values":{"phemios.duration":true}}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(
                address,
                "PATCH",
                route,
                Some(&one),
                r#"{"values":{"phemios.appearance":{"language":"en"}}}"#
            )
            .0,
            200
        );
        let own = http(address, "GET", route, Some(&one), "");
        assert_eq!(own.1["values"]["phemios.duration"], true);
        assert_eq!(
            http(address, "GET", route, Some(&two), "").1["values"],
            serde_json::json!({})
        );
        assert_eq!(
            http(
                address,
                "PATCH",
                route,
                Some(&auditor),
                r#"{"values":{"phemios.duration":false}}"#
            )
            .0,
            403
        );
        assert_eq!(
            http(
                address,
                "PATCH",
                route,
                Some(&one),
                r#"{"values":{"other.data":true}}"#
            )
            .0,
            400
        );
        assert_eq!(http(address, "GET", route, None, "").0, 401);
        server.abort();
    });
}

#[test]
fn private_engine_state_never_leaks_through_client_preferences() {
    let mut data = UserData::default();
    data.client_preferences.push(user::ClientPreference {
        owner: "one".into(),
        key: "aede.achievements.v1".into(),
        value: r#"{"version":1,"unlocked":{},"search":false}"#.into(),
    });
    data.client_preferences.push(user::ClientPreference {
        owner: "one".into(),
        key: "phemios.duration".into(),
        value: "true".into(),
    });
    assert_eq!(snapshot(&data, "one").unwrap().values.len(), 1);
    assert_eq!(
        snapshot(&data, "one").unwrap().values["phemios.duration"],
        true
    );
}

#[test]
fn application_preferences_cannot_replace_reserved_engine_progress_or_another_owners_values() {
    use crate::accounts_test_support::{Fixture, http, login};
    let runtime = crate::test_support::test_runtime();
    let fixture = Fixture::new();
    let (address, server) = runtime.block_on(crate::test_support::start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    assert_eq!(
        http(
            address,
            "POST",
            "/api/me/v1/achievements",
            Some(&alice),
            r#"{"advanced_search":true}"#
        )
        .0,
        200
    );
    assert_eq!(http(address, "PATCH", "/api/me/v1/preferences", Some(&bob), r#"{"values":{"phemios.podcasts":{"feeds":["https://example.invalid/feed.xml"]},"phemios.radio":{"stations":[]}}}"#).0, 200);
    let before = std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap();
    for body in [
        r#"{"values":{"aede.achievements.v1":{"version":1,"unlocked":{},"search":false}}}"#,
        r#"{"owner":"bob","values":{"phemios.radio":{"stations":[]}}}"#,
    ] {
        assert_eq!(
            http(
                address,
                "PATCH",
                "/api/me/v1/preferences",
                Some(&alice),
                body
            )
            .0,
            400
        );
        assert_eq!(
            std::fs::read(user::user_path(&fixture.0.data_dir)).unwrap(),
            before
        );
    }
    let (_, own, _) = http(address, "GET", "/api/me/v1/preferences", Some(&alice), "");
    assert_eq!(own["values"], serde_json::json!({}));
    let (_, progress, _) = http(address, "GET", "/api/me/v1/achievements", Some(&alice), "");
    assert_eq!(progress["unlocked"], 1);
    let (_, other, _) = http(address, "GET", "/api/me/v1/preferences", Some(&bob), "");
    assert_eq!(
        other["values"]["phemios.podcasts"]["feeds"][0],
        "https://example.invalid/feed.xml"
    );
    let (_, other_progress, _) = http(address, "GET", "/api/me/v1/achievements", Some(&bob), "");
    assert_eq!(other_progress["unlocked"], 0);
    server.abort();
}
