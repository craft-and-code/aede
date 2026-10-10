use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};
#[test]
fn successes_keep_owner_isolation_and_auditors_cannot_declare_actions() {
    let runtime = test_runtime();
    let fixture = Fixture::new();
    let (address, server) = runtime.block_on(start_server(fixture.0.clone()));
    let alice = login(address, "alice");
    let bob = login(address, "bob");
    let auditor = login(address, "auditor");
    let route = "/api/me/v1/achievements";
    let (status, answer, _) = http(
        address,
        "POST",
        route,
        Some(&alice),
        r#"{"advanced_search":true}"#,
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["unlocked"], 1);
    let (_, answer, _) = http(address, "GET", route, Some(&bob), "");
    assert_eq!(answer["unlocked"], 0);
    assert!(
        answer["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("title").is_none())
    );
    assert_eq!(http(address, "POST", route, Some(&auditor), "{}").0, 403);
    assert_eq!(http(address, "GET", route, None, "").0, 401);
    assert_eq!(
        http(address, "POST", route, Some(&alice), r#"{"owner":"bob"}"#).0,
        400
    );
    server.abort();
}
