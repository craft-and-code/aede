use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use axum::http::{Method, Request as HttpRequest};
use tower::ServiceExt;

#[tokio::test]
async fn original_export_is_byte_exact_private_revocable_and_does_not_record_listening() {
    let fixture = Fixture::new();
    let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
    {
        let mut c = fixture.0.catalog.write().await;
        c.as_mut().unwrap().roots = vec![fixture.0.data_dir.to_str().unwrap().into()];
    }
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    let reference = installed.reference.clone();
    let (token, plan) = tokio::task::spawn_blocking(move || {
        let alice = login(address, "alice");
        let auditor = login(address, "auditor");
        let payload = json!({"references":[reference]}).to_string();
        assert_eq!(
            http(address, "POST", "/api/me/v1/exports/plan", None, &payload).0,
            401
        );
        assert_eq!(
            http(
                address,
                "POST",
                "/api/me/v1/exports/plan",
                Some(&auditor),
                &payload
            )
            .0,
            403
        );
        let (status, plan, _) = http(
            address,
            "POST",
            "/api/me/v1/exports/plan",
            Some(&alice),
            &payload,
        );
        assert_eq!(status, 200, "{plan}");
        assert_eq!(
            plan["items"][0]["components"],
            json!(["remote-playback.wav"])
        );
        assert!(!plan.to_string().contains("a long test passphrase"));
        (alice, plan)
    })
    .await
    .unwrap();
    let stamp = plan["items"][0]["stamp"].as_str().unwrap();
    let path = format!(
        "/api/me/v1/exports/original?ref={}&stamp={}",
        crate::test_support::encoded(&installed.reference),
        crate::test_support::encoded(stamp)
    );
    let router = crate::routing::router(fixture.0.clone(), address);
    let request = || {
        HttpRequest::builder()
            .uri(&path)
            .header("Host", address.to_string())
            .header("Authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap()
    };
    let response = router.clone().oneshot(request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    assert_eq!(bytes.as_ref(), std::fs::read(&installed.path).unwrap());
    assert!(!aede_core::user::user_path(&fixture.0.data_dir).exists());
    let stale = path.replace(&crate::test_support::encoded(stamp), "obsolete");
    let stale_request = HttpRequest::builder()
        .uri(&stale)
        .header("Host", address.to_string())
        .header("Authorization", format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(
        router
            .clone()
            .oneshot(stale_request)
            .await
            .unwrap()
            .status(),
        StatusCode::CONFLICT
    );
    let logout = HttpRequest::builder()
        .method(Method::DELETE)
        .uri("/api/auth/v1/session")
        .header("Host", address.to_string())
        .header("Authorization", format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(logout).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        router.oneshot(request()).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    server.abort();
}

#[tokio::test]
async fn export_rejects_supervision_empty_duplicate_and_non_track_selections() {
    let fixture = Fixture::new();
    let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
    {
        let mut c = fixture.0.catalog.write().await;
        c.as_mut().unwrap().roots = vec![fixture.0.data_dir.to_str().unwrap().into()];
    }
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let admin = login(address, "operator");
        let alice = login(address, "alice");
        for references in [
            json!([]),
            json!([installed.reference, installed.reference]),
            json!(["artist:ac/dc"]),
        ] {
            assert_eq!(
                http(
                    address,
                    "POST",
                    "/api/me/v1/exports/plan",
                    Some(&alice),
                    &json!({"references":references}).to_string()
                )
                .0,
                400
            );
        }
        crate::parental_test_support::save_policy(
            address,
            &admin,
            "alice",
            crate::parental_test_support::policy(vec![]),
        );
        let payload = json!({"references":[installed.reference]}).to_string();
        assert_eq!(
            http(
                address,
                "POST",
                "/api/me/v1/exports/plan",
                Some(&alice),
                &payload
            )
            .0,
            403
        );
        assert_eq!(
            http(
                address,
                "GET",
                "/api/me/v1/exports/original?ref=track:unused&stamp=unused",
                Some(&alice),
                ""
            )
            .0,
            403
        );
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn original_export_stops_a_buffered_body_when_the_bearer_session_ends() {
    let fixture = Fixture::new();
    let installed = crate::playback_test_support::install_wav(&fixture, 131_072).await;
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    let token = tokio::task::spawn_blocking(move || login(address, "alice"))
        .await
        .unwrap();
    let principal = auth::principal(
        &fixture.0,
        &fixture.accounts(),
        &token,
        false,
        Instant::now(),
    )
    .unwrap();
    let source = {
        let held = fixture.0.catalog.read().await;
        playback_api::source_from_catalog(
            held.as_ref().unwrap(),
            &EntityRef::parse_token(&installed.reference).unwrap(),
        )
        .unwrap()
    };
    let response = crate::subsonic::media::response_for(
        fixture.0.clone(),
        crate::subsonic::media::MediaIdentity::Session(principal.clone()),
        EntityRef::parse_token(&installed.reference).unwrap(),
        HeaderMap::new(),
        true,
        Some(crate::subsonic::media::source_stamp(&source)),
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    auth::end_session(&fixture.0, &principal).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    assert!(
        axum::body::to_bytes(response.into_body(), 1_000_000)
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn export_preview_names_follow_conversion_policy_without_changing_original_identity() {
    let fixture = Fixture::new();
    let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
    {
        let mut catalog = fixture.0.catalog.write().await;
        catalog.as_mut().unwrap().roots = vec![fixture.0.data_dir.to_str().unwrap().into()];
    }
    let original = std::fs::read(&installed.path).unwrap();
    let (address, server) = crate::test_support::start_server(fixture.0.clone()).await;
    tokio::task::spawn_blocking(move || {
        let token = login(address, "alice");
        for (format, expected) in [
            ("mp3", "remote-playback.mp3"),
            ("aac", "remote-playback.m4a"),
            ("original", "remote-playback.wav"),
        ] {
            let (status, value, _) = http(
                address,
                "POST",
                "/api/me/v1/exports/plan",
                Some(&token),
                &json!({"references":[installed.reference],"format":format}).to_string(),
            );
            assert_eq!(status, 200, "{value}");
            assert_eq!(
                value["items"][0]["components"],
                json!(["remote-playback.wav"])
            );
            assert_eq!(value["items"][0]["output_components"], json!([expected]));
            assert!(value["items"][0]["output_key"].is_array());
            assert!(value["items"][0]["stamp"].as_str().is_some());
        }
        assert_eq!(
            http(
                address,
                "POST",
                "/api/me/v1/exports/plan",
                Some(&token),
                &json!({"references":[installed.reference],"format":"invalid"}).to_string()
            )
            .0,
            400
        );
        assert_eq!(std::fs::read(&installed.path).unwrap(), original);
    })
    .await
    .unwrap();
    server.abort();
}
