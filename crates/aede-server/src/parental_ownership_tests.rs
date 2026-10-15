use crate::accounts_test_support::{Fixture, PASSWORD, http, login};
use crate::parental_test_support::{policy, save_policy};
use serde_json::json;

#[tokio::test]
async fn supervising_administrator_exclusively_controls_rules_and_account_mutations() {
    let fixture = Fixture::new();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        assert_eq!(http(address, "POST", "/api/admin/v1/accounts", Some(&admin),
            &json!({"username":"second-admin","password":PASSWORD,"role":"admin"}).to_string()).0, 201);
        let other = login(address, "second-admin");
        let saved = save_policy(address, &admin, "alice", policy(vec![]));
        let admin_id = fixture.accounts().find("operator").unwrap().id.clone();
        assert_eq!(saved["locked_by"], admin_id);
        let route = "/api/admin/v1/accounts/alice/parental";
        assert_eq!(http(address, "GET", route, Some(&other), "").0, 200);
        let body = json!({"expected_owner":saved["owner"],"expected_revision":saved["revision"],"policy":saved["policy"]}).to_string();
        for credential in [&other, crate::accounts_test_support::ADMIN_TOKEN] {
            let (status, refused, _) = http(address, "PUT", route, Some(credential), &body);
            assert_eq!(status, 403, "{refused}");
            assert_eq!(refused["error"]["code"], "parental_locked");
            for change in [json!({"username":"renamed"}),json!({"role":"admin"}),json!({"enabled":false}),json!({"password":PASSWORD})] {
                assert_eq!(http(address, "PATCH", "/api/admin/v1/accounts/alice", Some(credential), &change.to_string()).0, 403);
            }
            assert_eq!(http(address, "DELETE", "/api/admin/v1/accounts/alice/sessions", Some(credential), "").0, 403);
        }
        // The controlling administrator cannot lose its authority and strand the lock.
        for credential in [&other, crate::accounts_test_support::ADMIN_TOKEN] {
            assert_eq!(http(address, "PATCH", "/api/admin/v1/accounts/operator", Some(credential),
                r#"{"password":"a replacement test passphrase"}"#).0, 403);
        }
        assert_eq!(http(address, "PATCH", "/api/admin/v1/accounts/operator", Some(&other), r#"{"role":"user"}"#).0, 403);
        let mut disabled = saved["policy"].clone(); disabled["enabled"] = json!(false);
        let released = save_policy(address, &admin, "alice", disabled);
        assert!(released["locked_by"].is_null());
        let reassigned = save_policy(address, &other, "alice", policy(vec![]));
        assert_ne!(reassigned["locked_by"], admin_id);
        assert_eq!(http(address, "PATCH", "/api/admin/v1/accounts/alice", Some(&admin), r#"{"role":"admin"}"#).0, 403);
    }).await.unwrap();
    server.abort();
}

#[tokio::test]
async fn global_podcast_refusal_applies_even_without_an_address_blacklist() {
    let fixture = Fixture::new();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator"); let user = login(address, "alice");
        let mut rules = policy(vec![]); rules["podcasts_allowed"] = json!(false);
        save_policy(address, &admin, "alice", rules.clone());
        let body = r#"{"action":"reserve","media":{"kind":"podcast","feed":"https://example.invalid/anything"},"seconds":10}"#;
        let (status, refused, _) = http(address,"POST","/api/me/v1/parental/lease",Some(&user),body);
        assert_eq!(status,403,"{refused}"); assert_eq!(refused["error"]["code"],"parental_podcast_blocked");
        rules["podcasts_allowed"] = json!(true); save_policy(address,&admin,"alice",rules);
        assert_eq!(http(address,"POST","/api/me/v1/parental/lease",Some(&user),body).0,200);
    }).await.unwrap();
    server.abort();
}
