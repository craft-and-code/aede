use super::*;
use crate::accounts_test_support::{Fixture, PASSWORD};

#[test]
fn sessions_expire_by_idle_and_absolute_time_without_notification_refresh() {
    let fixture = Fixture::new();
    let data = fixture.accounts();
    let account = data.find("alice").unwrap();
    let (session, _) = start_session(&fixture.0, &data, account).unwrap();
    let now = Instant::now();
    assert!(
        principal(
            &fixture.0,
            &data,
            &session.token,
            false,
            now + IDLE_LIFETIME
        )
        .is_err()
    );
    let (session, _) = start_session(&fixture.0, &data, account).unwrap();
    for minute in (20..720).step_by(20) {
        assert!(
            principal(
                &fixture.0,
                &data,
                &session.token,
                true,
                now + Duration::from_secs(minute * 60)
            )
            .is_ok()
        );
    }
    assert!(
        principal(
            &fixture.0,
            &data,
            &session.token,
            true,
            now + ABSOLUTE_LIFETIME + Duration::from_secs(1)
        )
        .is_err()
    );
}

#[test]
fn session_eviction_is_per_owner_and_revocation_is_durable() {
    let fixture = Fixture::new();
    let mut data = fixture.accounts();
    let (bob, _) = start_session(&fixture.0, &data, data.find("bob").unwrap()).unwrap();
    let (oldest, _) = start_session(&fixture.0, &data, data.find("alice").unwrap()).unwrap();
    for _ in 0..8 {
        start_session(&fixture.0, &data, data.find("alice").unwrap()).unwrap();
    }
    assert!(principal(&fixture.0, &data, &oldest.token, false, Instant::now()).is_err());
    assert!(principal(&fixture.0, &data, &bob.token, false, Instant::now()).is_ok());
    data.set_password("bob", PASSWORD, 20).unwrap();
    assert!(principal(&fixture.0, &data, &bob.token, false, Instant::now()).is_err());
    assert_eq!(fixture.0.auth.sessions.lock().unwrap().len(), 8);
}

#[test]
fn throttling_limits_unknown_names_and_recovers_without_unbounded_storage() {
    let now = Instant::now();
    let mut attempts = Attempts::default();
    for _ in 0..5 {
        attempts.take("alice", now).unwrap();
    }
    assert_eq!(
        attempts.take("alice", now).unwrap_err().status,
        StatusCode::TOO_MANY_REQUESTS
    );
    for i in 0..95 {
        attempts.take(&format!("unknown-{i}"), now).unwrap();
    }
    assert!(attempts.take("another", now).is_err());
    attempts.take("alice", now + RATE_WINDOW).unwrap();
    assert_eq!(attempts.names.len(), 1);
}

#[test]
fn missing_or_malformed_credentials_fail_closed_until_a_fresh_local_server_starts() {
    let fixture = Fixture::new();
    assert!(load_accounts(&fixture.0).unwrap().is_some());
    std::fs::remove_file(accounts::accounts_path(&fixture.0.data_dir)).unwrap();
    assert_eq!(
        load_accounts(&fixture.0).unwrap_err().status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let mut restarted = fixture.0.clone();
    restarted.auth = Arc::new(AuthState::default());
    assert!(
        load_accounts(&restarted).unwrap().is_none(),
        "an initially absent store retains the local anonymous compatibility mode"
    );
    let second = Fixture::new();
    std::fs::write(accounts::accounts_path(&second.0.data_dir), "{}").unwrap();
    assert!(load_accounts(&second.0).is_err());
    std::fs::remove_file(accounts::accounts_path(&second.0.data_dir)).unwrap();
    assert!(load_accounts(&second.0).is_err());
}

#[test]
fn bearer_credentials_reject_duplicate_headers_and_query_tokens() {
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, "Bearer first".parse().unwrap());
    assert_eq!(bearer(&headers), Some("first"));
    headers.append(header::AUTHORIZATION, "Bearer second".parse().unwrap());
    assert!(bearer(&headers).is_none());
    assert!(!token_matches(&"a".repeat(64), &"b".repeat(64)));
    assert!(!token_matches("short", "short"));
}

#[test]
fn captured_identity_is_refused_after_an_operator_revokes_it() {
    let fixture = Fixture::new();
    let mut data = fixture.accounts();
    let (session, _) = start_session(&fixture.0, &data, data.find("alice").unwrap()).unwrap();
    data.revoke("alice", 20).unwrap();
    accounts::save(&data, &accounts::accounts_path(&fixture.0.data_dir)).unwrap();
    assert!(recheck(&fixture.0, &session).is_err());
}

#[test]
fn the_global_session_limit_refuses_a_new_owner_without_evicting_other_accounts() {
    let fixture = Fixture::new();
    let mut document = accounts::to_json(&fixture.accounts());
    let template = document.get("account").unwrap().as_arr().unwrap()[1].clone();
    let mut rows = document.get("account").unwrap().as_arr().unwrap().to_vec();
    for index in 0..33 {
        let mut row = template.clone();
        row.set("id", format!("account-{:064x}", index + 1).into());
        row.set("username", format!("listener-{index}").into());
        rows.push(row);
    }
    document.set("account", aede_core::json::Json::Arr(rows));
    let mut data = accounts::from_json(&document).unwrap();
    for index in 0..32 {
        for _ in 0..8 {
            start_session(
                &fixture.0,
                &data,
                data.find(&format!("listener-{index}")).unwrap(),
            )
            .unwrap();
        }
    }
    assert_eq!(
        start_session(&fixture.0, &data, data.find("listener-32").unwrap())
            .err()
            .unwrap()
            .code,
        "session_limit"
    );
    assert_eq!(fixture.0.auth.sessions.lock().unwrap().len(), MAX_SESSIONS);
    data.revoke("listener-0", 20).unwrap();
    assert!(
        start_session(&fixture.0, &data, data.find("listener-32").unwrap()).is_ok(),
        "revoked sessions must release global capacity"
    );
}
