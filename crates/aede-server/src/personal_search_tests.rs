use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};
#[test]
fn advanced_search_retains_owner_scope_and_refuses_unknown_fields() {
    let runtime = test_runtime();
    let fixture = Fixture::new();
    let catalog = fixture.0.catalog.blocking_read().clone().unwrap();
    let token = EntityRef::of(&catalog, EntityKind::Track, 0)
        .unwrap()
        .to_token();
    let (address, server) = runtime.block_on(start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    let auditor = login(address, "auditor");
    let target = format!(
        "/api/me/v1/annotation?ref={}",
        crate::test_support::encoded(&token)
    );
    assert_eq!(
        http(
            address,
            "PUT",
            &target,
            Some(&alice),
            r#"{"rating":4,"loved":true}"#
        )
        .0,
        200
    );
    let route = "/api/me/v1/search?expression=loved%20rating:4&limit=50";
    let (status, answer, _) = http(address, "GET", route, Some(&alice), "");
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["total"], 1);
    assert_eq!(answer["items"][0]["reference"], token);
    assert_eq!(http(address, "GET", route, Some(&bob), "").1["total"], 0);
    assert_eq!(http(address, "GET", route, Some(&auditor), "").0, 200);
    assert_eq!(http(address, "GET", route, None, "").0, 401);
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/search?expression=bitdepth:NaN",
            Some(&alice),
            ""
        )
        .0,
        400
    );
    assert_eq!(
        http(
            address,
            "GET",
            "/api/me/v1/search?expression=loved&owner=bob",
            Some(&alice),
            ""
        )
        .0,
        400
    );
    server.abort();
}
