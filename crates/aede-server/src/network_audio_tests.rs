use super::*;

#[test]
fn shutdown_during_network_source_preparation_cannot_admit_playback() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for source_wait in [false, true] {
            let fixture = crate::accounts_test_support::Fixture::new();
            let installed = crate::playback_test_support::install_wav(&fixture, 400).await;
            let accounts = fixture.accounts();
            let (principal, _) =
                auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap())
                    .unwrap();
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/admin/v1/network-audio/play")
                .header(
                    "authorization",
                    format!("Bearer {}", auth::session_token(&principal)),
                )
                .body(axum::body::Body::from(
                    serde_json::json!({
                        "bind": "127.0.0.1",
                        "device": "http://127.0.0.1:1/description.xml",
                        "tracks": [installed.reference],
                    })
                    .to_string(),
                ))
                .unwrap();
            request.extensions_mut().insert(principal);
            let _shutdown_observer = fixture.0.shutdown.subscribe();
            let catalog = if source_wait {
                None
            } else {
                Some(fixture.0.catalog.write().await)
            };
            let (release, worker) = crate::test_support::hold_blocking_worker().await;
            let mut playback = Box::pin(play(State(fixture.0.clone()), request));
            std::future::poll_fn(|context| {
                assert!(playback.as_mut().poll(context).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            drop(release);
            worker.await.unwrap();
            tokio::task::spawn_blocking(|| ()).await.unwrap();
            let validation = if source_wait {
                Some(crate::test_support::hold_blocking_worker().await)
            } else {
                None
            };
            std::future::poll_fn(|context| {
                assert!(playback.as_mut().poll(context).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            fixture.0.shutdown.send(()).unwrap();
            drop(catalog);
            if let Some((release, worker)) = validation {
                drop(release);
                worker.await.unwrap();
            }
            let result = playback.await;
            let admitted = {
                let held = fixture.0.network_audio.run.lock().unwrap();
                if let Some(run) = held.as_ref() {
                    // The current-thread fixture cancels before the spawned
                    // supervisor can receive its first poll or contact a device.
                    run.cancelled.store(true, Ordering::Release);
                }
                held.is_some()
            };
            if admitted {
                tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        if fixture
                            .0
                            .network_audio
                            .run
                            .lock()
                            .unwrap()
                            .as_ref()
                            .is_none_or(|run| !matches!(run.status, "running" | "stopping"))
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
            }
            assert!(
                result.is_err() && !admitted,
                "shutdown must refuse admission (source_wait={source_wait})"
            );
            assert!(!aede_core::user::user_path(&fixture.0.data_dir).exists());
        }
    });
}

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
        fixture.0.shutdown.subscribe(),
    );
    drop(supervision);
    assert!(flag.load(Ordering::Acquire));
}

#[test]
fn queued_network_authentication_cannot_delay_shutdown_or_lease_expiry() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut delayed = Vec::new();
        for shutdown in [true, false] {
            let fixture = crate::accounts_test_support::Fixture::new();
            let accounts = fixture.accounts();
            let principal =
                auth::start_session(&fixture.0, &accounts, accounts.find("operator").unwrap())
                    .unwrap()
                    .0;
            let source = crate::playback_test_support::install_wav(&fixture, 400).await;
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
                device: "http://127.0.0.1:1/description.xml".into(),
                port: None,
                replace: false,
                volume: None,
                certificate_sha256: None,
            };
            let (release, worker) = crate::test_support::hold_blocking_worker().await;
            let mut supervision = Box::pin(supervise(
                fixture.0.clone(),
                principal,
                7,
                cancelled.clone(),
                options,
                vec![source.path],
                fixture.0.shutdown.subscribe(),
            ));
            poll_pending_supervision(supervision.as_mut()).await;
            // Let the immediate authorization tick become ready, then queue
            // its credential worker behind the held blocking pool.
            tokio::time::sleep(Duration::from_millis(1)).await;
            poll_pending_supervision(supervision.as_mut()).await;
            assert!(!cancelled.load(Ordering::Acquire));
            if shutdown {
                fixture.0.shutdown.send(()).unwrap();
            } else {
                fixture
                    .0
                    .network_audio
                    .run
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .last_seen = Instant::now() - Duration::from_secs(6);
                tokio::time::sleep(Duration::from_millis(550)).await;
            }
            poll_pending_supervision(supervision.as_mut()).await;
            if !cancelled.load(Ordering::Acquire) {
                delayed.push(if shutdown { "shutdown" } else { "lease expiry" });
            }
            // Drain queued work only after observing the cancellation boundary.
            // No real renderer is contacted, even on regression failure.
            cancelled.store(true, Ordering::Release);
            drop(release);
            worker.await.unwrap();
            tokio::time::timeout(Duration::from_secs(3), supervision)
                .await
                .unwrap();
        }
        assert!(
            delayed.is_empty(),
            "pending credential work delayed cancellation: {delayed:?}"
        );
    });
}

async fn poll_pending_supervision(mut future: std::pin::Pin<&mut impl Future<Output = ()>>) {
    std::future::poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
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
            fixture.0.shutdown.subscribe(),
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
