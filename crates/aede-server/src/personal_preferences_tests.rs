use super::*;
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
