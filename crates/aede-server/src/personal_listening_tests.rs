use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};
use aede_core::{model::EntityKind, user::EntityRef};
#[test]
fn completion_pages_are_owner_private_and_require_all_catalogued_tracks() {
    let runtime = test_runtime();
    let fixture = Fixture::new();
    let tracks = {
        let guard = fixture.0.catalog.try_read().unwrap();
        let catalog = guard.as_ref().unwrap();
        catalog
            .tracks
            .iter()
            .map(|track| {
                EntityRef::of(catalog, EntityKind::Track, track.id)
                    .unwrap()
                    .to_token()
            })
            .collect::<Vec<_>>()
    };
    let (address, server) = runtime.block_on(start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    let auditor = login(address, "auditor");
    let route = "/api/me/v1/album-listening?limit=200";
    let (status, initial, _) = http(address, "GET", route, Some(&alice), "");
    assert_eq!(status, 200, "{initial}");
    assert!(initial["total"].as_u64().unwrap() > 0);
    assert!(
        initial["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["completed"] == false)
    );
    for track in &tracks {
        let body = serde_json::json!({"track":track,"ms_played":1000,"completed":true}).to_string();
        assert_eq!(
            http(address, "POST", "/api/me/v1/history", Some(&alice), &body).0,
            201
        );
    }
    let (_, finished, _) = http(address, "GET", route, Some(&alice), "");
    assert!(
        finished["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["completed"] == true)
    );
    for token in [&bob, &auditor] {
        let (status, rows, _) = http(address, "GET", route, Some(token), "");
        assert_eq!(status, 200);
        assert!(
            rows["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["completed"] == false)
        );
    }
    assert_eq!(http(address, "GET", route, None, "").0, 401);
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/album-listening?owner=bob",
            Some(&alice),
            ""
        )
        .0,
        400
    );
    assert_eq!(http(address, "GET", route, Some(&alice), "{}").0, 400);
    assert_eq!(
        http(
            address,
            "POST",
            "/api/me/v1/album-listening",
            Some(&alice),
            "{}"
        )
        .0,
        405
    );
    assert_eq!(
        http(
            address,
            "DELETE",
            "/api/me/v1/history",
            Some(&alice),
            r#"{"confirm":true}"#
        )
        .0,
        200
    );
    let (_, reset, _) = http(address, "GET", route, Some(&alice), "");
    assert!(
        reset["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["completed"] == false)
    );
    server.abort();
}
