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
fn aborted_supervision_requests_transport_shutdown() {
    let flag = Arc::new(AtomicBool::new(false));
    {
        let _guard = CancellationGuard(flag.clone());
    }
    assert!(flag.load(Ordering::Acquire));
}
