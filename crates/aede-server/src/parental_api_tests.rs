use crate::accounts_test_support::{Fixture, http, login};
use crate::parental_test_support::{policy, save_policy};
use serde_json::json;

#[tokio::test]
async fn admin_policy_hides_catalogue_and_private_history_without_changing_another_account() {
    let fixture = Fixture::new();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        let alice = login(address, "alice");
        let bob = login(address, "bob");
        let track = "track:/music/album/01.flac";
        let target = format!(
            "/api/me/v1/annotation?ref={}",
            crate::test_support::encoded(track)
        );
        assert_eq!(
            http(
                address,
                "PUT",
                &target,
                Some(&alice),
                r#"{"rating":4,"loved":true,"note":"Retained private note"}"#
            )
            .0,
            200
        );
        assert_eq!(
            http(
                address,
                "POST",
                "/api/me/v1/history",
                Some(&alice),
                &json!({"track":track,"ms_played":500,"completed":false}).to_string()
            )
            .0,
            201
        );
        let route = "/api/admin/v1/accounts/alice/parental";
        assert_eq!(http(address, "GET", route, Some(&alice), "").0, 403);
        let (status, initial, _) = http(address, "GET", route, Some(&admin), "");
        assert_eq!(status, 200, "{initial}");
        let stale_owner = json!({"expected_owner":"replaced-account","expected_revision":initial["revision"],"policy":policy(vec![])});
        assert_eq!(http(address, "PUT", route, Some(&admin), &stale_owner.to_string()).0, 409);
        let body =
            json!({"expected_owner":initial["owner"],"expected_revision":initial["revision"],"policy":policy(vec!["artist:ac/dc"])})
                .to_string();
        let (status, saved, _) = http(address, "PUT", route, Some(&admin), &body);
        assert_eq!(status, 200, "{saved}");
        assert_eq!(http(address, "PUT", route, Some(&admin), &body).0, 409);
        let (_, page, _) = http(
            address,
            "GET",
            "/api/v1/presentation/albums",
            Some(&alice),
            "",
        );
        assert_eq!(page["total"], 0, "{page}");
        assert_eq!(
            http(address, "GET", "/api/me/v1/history", Some(&alice), "").1["total"],
            0
        );
        assert_eq!(
            http(address, "GET", "/api/me/v1/annotations", Some(&alice), "").1["total"],
            0
        );
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/search?expression=loved",
                Some(&alice),
                ""
            )
            .1["total"],
            0
        );
        assert_eq!(http(address, "GET", &target, Some(&alice), "").0, 404);
        for endpoint in ["/api/v1/roots", "/api/me/v1/m3u-files", "/api/v1/activity"] {
            assert_eq!(
                http(address, "GET", endpoint, Some(&alice), "").0,
                403,
                "{endpoint}"
            );
        }

        let (_, page, _) = http(
            address,
            "GET",
            "/api/v1/presentation/albums",
            Some(&bob),
            "",
        );
        assert_eq!(page["total"], 1, "{page}");
        let (status, state, _) = http(address, "GET", "/api/me/v1/parental", Some(&alice), "");
        assert_eq!(status, 200, "{state}");
        assert_eq!(state["policy"]["enabled"], true);
        let (_, session, _) = http(address, "GET", "/api/auth/v1/session", Some(&alice), "");
        assert_eq!(session["parental_enabled"], true);
        let mut disabled=saved["policy"].clone();disabled["enabled"]=json!(false);
        save_policy(address,&admin,"alice",disabled);
        assert_eq!(http(address,"GET",&target,Some(&alice),"").1["annotation"]["note"],"Retained private note");
        assert_eq!(http(address,"GET","/api/me/v1/history",Some(&alice),"").1["total"],1);
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn external_leases_require_their_owner_and_refuse_blocked_media_and_invalid_admin_targets() {
    let fixture = Fixture::new();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin=login(address,"operator");let alice=login(address,"alice");let bob=login(address,"bob");
        let mut rules=policy(vec![]);rules["blocked_podcasts"]=json!(["https://example.invalid/show.xml"]);
        save_policy(address,&admin,"alice",rules.clone());
        let endpoint="/api/me/v1/parental/lease";
        let (status,answer,_)=http(address,"POST",endpoint,Some(&alice),r#"{"action":"reserve","media":{"kind":"radio"},"seconds":10}"#);
        assert_eq!(status,403,"{answer}");assert_eq!(answer["error"]["code"],"parental_radio_blocked");
        let (status,answer,_)=http(address,"POST",endpoint,Some(&alice),r#"{"action":"reserve","media":{"kind":"podcast","feed":"https://EXAMPLE.invalid:443/show.xml"},"seconds":10}"#);
        assert_eq!(status,403,"{answer}");assert_eq!(answer["error"]["code"],"parental_podcast_blocked");
        rules["radio_allowed"]=json!(true);save_policy(address,&admin,"alice",rules);
        let (status,grant,_)=http(address,"POST",endpoint,Some(&alice),r#"{"action":"reserve","media":{"kind":"radio"},"seconds":10}"#);
        assert_eq!(status,200,"{grant}");assert_eq!(grant["granted_seconds"],10); assert!(grant["server_now_ms"].as_u64().is_some_and(|ms| ms > 0));
        let release=json!({"action":"release","lease_id":grant["lease_id"],"consumed_ms":0}).to_string();
        assert_ne!(http(address,"POST",endpoint,Some(&bob),&release).0,204);
        assert_eq!(http(address,"POST",endpoint,Some(&alice),&release).0,204);
        assert_ne!(http(address,"POST",endpoint,Some(&alice),&release).0,204);
        let (status,_,_)=http(address,"PUT","/api/admin/v1/accounts/operator/parental",Some(&admin),&json!({"expected_owner":"operator","expected_revision":0,"policy":policy(vec![])}).to_string());
        assert_eq!(status,400);
        let (_,session,_)=http(address,"GET","/api/auth/v1/session",Some(&bob),"");assert_eq!(session["parental_enabled"],false);
    }).await.unwrap();
    server.abort();
}

#[tokio::test]
async fn restored_policy_with_the_same_revision_does_not_reuse_a_different_visible_graph() {
    let fixture = Fixture::new();
    let owner = fixture.accounts().find("alice").unwrap().id.clone();
    let mut data = aede_core::parental::ParentalStore::default();
    data.set_policy(
        &owner,
        serde_json::from_value(policy(vec!["artist:ac/dc"])).unwrap(),
        0,
    )
    .unwrap();
    data.save(&aede_core::parental::path(&fixture.0.data_dir))
        .unwrap();
    let hidden = crate::parental::scoped(fixture.0.clone(), owner.clone(), false, true)
        .await
        .unwrap();
    assert_eq!(
        hidden.catalog.read().await.as_ref().unwrap().releases.len(),
        0
    );
    let mut restored = aede_core::parental::ParentalStore::default();
    restored
        .set_policy(
            &owner,
            serde_json::from_value(policy(vec!["artist:another artist"])).unwrap(),
            0,
        )
        .unwrap();
    restored
        .save(&aede_core::parental::path(&fixture.0.data_dir))
        .unwrap();
    let visible = crate::parental::scoped(fixture.0.clone(), owner, false, true)
        .await
        .unwrap();
    assert_eq!(
        visible
            .catalog
            .read()
            .await
            .as_ref()
            .unwrap()
            .releases
            .len(),
        1
    );
    assert!(crate::parental::confirm(&hidden).is_err());
}

#[tokio::test]
async fn disappearance_of_an_observed_parental_store_never_reopens_a_supervised_account() {
    let fixture = Fixture::new();
    let owner = fixture.accounts().find("alice").unwrap().id.clone();
    let mut data = aede_core::parental::ParentalStore::default();
    data.set_policy(&owner, serde_json::from_value(policy(vec![])).unwrap(), 0)
        .unwrap();
    let path = aede_core::parental::path(&fixture.0.data_dir);
    data.save(&path).unwrap();
    assert!(
        crate::parental::scoped(fixture.0.clone(), owner.clone(), false, true)
            .await
            .is_ok()
    );
    std::fs::remove_file(path).unwrap();
    assert!(
        crate::parental::scoped(fixture.0.clone(), owner, false, true)
            .await
            .is_err()
    );
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        assert_eq!(
            http(address, "GET", "/api/auth/v1/session", Some(&admin), "").1["parental_enabled"],
            false
        );
    })
    .await
    .unwrap();
    server.abort();
}

#[test]
fn podcast_policy_addresses_are_canonical_http_uris_without_credentials() {
    let canonical =
        crate::parental::http::canonical_feed("HTTPS://EXAMPLE.invalid:443/show.xml?episode=1")
            .unwrap();
    assert_eq!(canonical, "https://example.invalid/show.xml?episode=1");
    for invalid in [
        "file:///private/show.xml",
        "https://user:secret@example.invalid/show.xml",
        "not a URL",
    ] {
        assert!(crate::parental::http::canonical_feed(invalid).is_err());
    }
}
