//! Fictional policies and authenticated local requests shared by parental tests.
use crate::accounts_test_support::http;
use serde_json::{Value, json};

pub(crate) fn policy(blocked: Vec<&str>) -> Value {
    json!({"enabled":true,"timezone":"UTC","blocked_artists":blocked,"radio_allowed":false,"blocked_podcasts":[],
        "days":(0..7).map(|_|json!({"windows":[{"start_minute":0,"end_minute":1440}],"limit_seconds":null})).collect::<Vec<_>>(),"week_limit_seconds":null})
}

pub(crate) fn save_policy(
    address: std::net::SocketAddr,
    admin: &str,
    name: &str,
    policy: Value,
) -> Value {
    let route = format!("/api/admin/v1/accounts/{name}/parental");
    let (status, before, _) = http(address, "GET", &route, Some(admin), "");
    assert_eq!(status, 200, "{before}");
    let (status, after, _) = http(
        address,
        "PUT",
        &route,
        Some(admin),
        &json!({"expected_owner":before["owner"],"expected_revision":before["revision"],"policy":policy}).to_string(),
    );
    assert_eq!(status, 200, "{after}");
    after
}
