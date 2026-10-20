use super::*;
#[test]
fn network_sessions_are_visible_only_to_the_exact_login() {
    let registry = NetworkAudio::default();
    *registry.run.lock().unwrap() = Some(Run {
        id: 7,
        owner: "first-login".into(),
        cancelled: Arc::new(AtomicBool::new(false)),
        status: "running",
        last_seen: Instant::now(),
    });
    assert_eq!(registry.snapshot("first-login").unwrap()["id"], 7);
    assert_eq!(
        registry.snapshot("other-login").unwrap(),
        serde_json::Value::Null
    );
}
#[test]
fn discovery_has_one_real_slot_and_refuses_overlapping_work() {
    let registry = NetworkAudio::default();
    let permit = registry.discovery.clone().try_acquire_owned().unwrap();
    assert!(registry.discovery.clone().try_acquire_owned().is_err());
    drop(permit);
    assert!(registry.discovery.clone().try_acquire_owned().is_ok());
}
#[test]
fn network_requests_refuse_unknown_fields_and_invalid_addresses() {
    assert!(decode::<Discovery>(br#"{"bind":"0.0.0.0","extra":true}"#).is_err());
    assert!(decode::<Discovery>(br#"{"bind":"example.org"}"#).is_err());
    assert!(decode::<Stop>(br#"{"id":-1}"#).is_err());
    assert!(decode::<Playback>(br#"{"bind":"127.0.0.1","device":"http://127.0.0.1/device.xml","tracks":[],"path":"/outside"}"#).is_err());
}

#[test]
fn stopping_requires_the_exact_session_and_waits_for_worker_confirmation() {
    let registry = NetworkAudio::default();
    let flag = Arc::new(AtomicBool::new(false));
    *registry.run.lock().unwrap() = Some(Run {
        id: 7,
        owner: "first-login".into(),
        cancelled: flag.clone(),
        status: "running",
        last_seen: Instant::now(),
    });
    assert!(registry.stop("another-login", 7).is_err());
    assert!(registry.stop("first-login", 8).is_err());
    assert!(!flag.load(Ordering::Acquire));
    assert_eq!(
        registry.stop("first-login", 7).unwrap()["status"],
        "stopping"
    );
    assert!(flag.load(Ordering::Acquire));
    assert_eq!(
        registry.snapshot("first-login").unwrap()["status"],
        "stopping"
    );
}
#[test]
fn only_the_owner_heartbeat_renews_the_bounded_playback_lease() {
    let registry = NetworkAudio::default();
    *registry.run.lock().unwrap() = Some(Run {
        id: 7,
        owner: "first-login".into(),
        cancelled: Arc::new(AtomicBool::new(false)),
        status: "running",
        last_seen: Instant::now() - Duration::from_secs(6),
    });
    assert!(registry.expired(7));
    registry.snapshot("other-login").unwrap();
    assert!(registry.expired(7));
    registry.snapshot("first-login").unwrap();
    assert!(!registry.expired(7));
    assert!(registry.expired(8));
}

#[test]
fn network_audio_requires_a_named_administrator_not_a_legacy_token_or_user() {
    use crate::accounts_test_support::{ADMIN_TOKEN, Fixture};
    let fixture = Fixture::new();
    let accounts = fixture.accounts();
    let legacy = Request::builder()
        .uri("/api/admin/v1/network-audio")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert!(owner(&fixture.0, &legacy).is_err());
    for (name, accepted) in [("alice", false), ("auditor", false), ("operator", true)] {
        let (principal, _) =
            auth::start_session(&fixture.0, &accounts, accounts.find(name).unwrap()).unwrap();
        let mut request = Request::builder()
            .uri("/api/admin/v1/network-audio")
            .body(axum::body::Body::empty())
            .unwrap();
        request.extensions_mut().insert(principal);
        assert_eq!(owner(&fixture.0, &request).is_ok(), accepted);
    }
}

#[test]
fn dropping_unpolled_supervision_requests_transport_shutdown() {
    let fixture = crate::accounts_test_support::Fixture::new();
    let accounts = fixture.accounts();
    let (principal, _) =
        auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap()).unwrap();
    let flag = Arc::new(AtomicBool::new(false));
    let options = aede_devices::CastOptions {
        bind: Ipv4Addr::LOCALHOST,
        protocol: aede_devices::DeviceProtocol::Upnp,
        device: "http://127.0.0.1:1/description.xml".into(),
        port: None,
        replace: false,
        volume: None,
        certificate_sha256: None,
    };
    let supervision = supervise(
        fixture.0.clone(),
        principal,
        7,
        flag.clone(),
        options,
        Vec::new(),
    );
    drop(supervision);
    assert!(flag.load(Ordering::Acquire));
}

#[test]
fn shutdown_before_network_supervision_first_poll_cancels_accepted_playback() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = crate::accounts_test_support::Fixture::new();
        let accounts = fixture.accounts();
        let (principal, _) =
            auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap()).unwrap();
        let source = crate::playback_test_support::install_wav(&fixture, 400).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        *fixture.0.network_audio.run.lock().unwrap() = Some(Run {
            id: 7,
            owner: auth::session_token(&principal).into(),
            cancelled: cancelled.clone(),
            status: "running",
            last_seen: Instant::now(),
        });
        let options = aede_devices::CastOptions {
            bind: Ipv4Addr::LOCALHOST,
            protocol: aede_devices::DeviceProtocol::Upnp,
            device: format!("http://{}/description.xml", listener.local_addr().unwrap()),
            port: None,
            replace: false,
            volume: None,
            certificate_sha256: None,
        };
        // Another server listener receives the shutdown even if supervision
        // has not yet subscribed. The accepted playback must receive it too.
        let _other_listener = fixture.0.shutdown.subscribe();
        let supervision = supervise(
            fixture.0.clone(),
            principal,
            7,
            cancelled.clone(),
            options,
            vec![source.path],
        );
        fixture.0.shutdown.send(()).unwrap();
        let mut task = tokio::spawn(supervision);
        let observed = tokio::time::timeout(Duration::from_secs(1), async {
            while !cancelled.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        // Release the real transport even when the regression fails.
        cancelled.store(true, Ordering::Release);
        let completed = tokio::time::timeout(Duration::from_secs(3), &mut task).await;
        if completed.is_err() {
            task.abort();
        }
        completed
            .expect("transport did not finish cancellation")
            .expect("supervision panicked");
        assert!(
            observed.is_ok(),
            "shutdown sent before the first poll was lost"
        );
        assert_eq!(
            listener.into_std().unwrap().accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "shutdown before the first poll must prevent device contact",
        );
    });
}

#[test]
fn logout_during_network_status_cannot_publish_or_extend_a_playback_lease() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = crate::accounts_test_support::Fixture::new();
        let mut request = crate::admin_session_test_support::signed_request(&fixture, "");
        *request.method_mut() = axum::http::Method::GET;
        *request.uri_mut() = "/api/admin/v1/network-audio".parse().unwrap();
        let principal = request.extensions().get::<auth::Principal>().unwrap();
        let seen = Instant::now() - Duration::from_secs(6);
        *fixture.0.network_audio.run.lock().unwrap() = Some(Run {
            id: 7,
            owner: auth::session_token(principal).into(),
            cancelled: Arc::new(AtomicBool::new(false)),
            status: "running",
            last_seen: seen,
        });
        let result = status(State(fixture.0.clone()), request).await;
        assert_eq!(
            result
                .expect_err("revoked logins cannot see or renew a network run")
                .status,
            StatusCode::UNAUTHORIZED,
        );
        let held = fixture.0.network_audio.run.lock().unwrap();
        assert_eq!(held.as_ref().unwrap().last_seen, seen);
    });
}

#[test]
fn logout_during_network_stop_cannot_change_a_playback_run() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = crate::accounts_test_support::Fixture::new();
        let mut request =
            crate::admin_session_test_support::signed_request(&fixture, r#"{"id":7}"#);
        *request.uri_mut() = "/api/admin/v1/network-audio/stop".parse().unwrap();
        let principal = request.extensions().get::<auth::Principal>().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        *fixture.0.network_audio.run.lock().unwrap() = Some(Run {
            id: 7,
            owner: auth::session_token(principal).into(),
            cancelled: cancelled.clone(),
            status: "running",
            last_seen: Instant::now(),
        });
        let result = stop(State(fixture.0.clone()), request).await;
        assert_eq!(
            result
                .expect_err("revoked logins cannot dispatch a stop")
                .status,
            StatusCode::UNAUTHORIZED,
        );
        assert!(!cancelled.load(Ordering::Acquire));
        let held = fixture.0.network_audio.run.lock().unwrap();
        assert_eq!(held.as_ref().unwrap().status, "running");
    });
}

#[test]
fn logout_during_device_request_admission_cannot_reach_the_discovery_slot() {
    crate::test_support::test_runtime().block_on(async {
        for operation in ["discover", "inspect"] {
            let fixture = crate::accounts_test_support::Fixture::new();
            let _permit = fixture
                .0
                .network_audio
                .discovery
                .clone()
                .try_acquire_owned()
                .unwrap();
            let payload = match operation {
                "discover" => r#"{"bind":"127.0.0.1"}"#,
                _ => r#"{"bind":"127.0.0.1","device":"127.0.0.1:1"}"#,
            };
            let mut request = crate::admin_session_test_support::signed_request(&fixture, payload);
            *request.uri_mut() = format!("/api/admin/v1/network-audio/{operation}")
                .parse()
                .unwrap();
            let result = if operation == "discover" {
                discover(State(fixture.0.clone()), request).await
            } else {
                inspect(State(fixture.0.clone()), request).await
            };
            assert_eq!(
                result
                    .expect_err("revoked requests must stop before device admission")
                    .status,
                StatusCode::UNAUTHORIZED,
                "{operation}",
            );
        }
    });
}
