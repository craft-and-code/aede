use super::*;
use crate::accounts_test_support::{Fixture, PASSWORD, http};
use crate::test_support::{start_server, test_runtime};

#[test]
fn a_failed_login_projection_does_not_allocate_or_evict_account_sessions() {
    test_runtime().block_on(async {
        let mut changed = Vec::new();
        for existing in [0, MAX_ACCOUNT_SESSIONS] {
            let fixture = Fixture::new();
            let data = fixture.accounts();
            for _ in 0..existing {
                start_session(&fixture.0, &data, data.find("alice").unwrap()).unwrap();
            }
            start_session(&fixture.0, &data, data.find("bob").unwrap()).unwrap();
            let before: Vec<_> = fixture
                .0
                .auth
                .sessions
                .lock()
                .unwrap()
                .iter()
                .map(|session| session.principal.token.clone())
                .collect();
            let policy_path = aede_core::parental::path(&fixture.0.data_dir);
            aede_core::parental::ParentalStore::default().save(&policy_path).unwrap();
            // Preserve the private mode while making the account projection unreadable.
            std::fs::write(&policy_path, b"{").unwrap();
            let (address, server) = start_server(fixture.0.clone()).await;
            let (status, answer, _) = http(
                address,
                "POST",
                "/api/auth/v1/session",
                None,
                &format!(r#"{{"username":"alice","password":"{PASSWORD}"}}"#),
            );
            server.abort();
            assert_eq!(status, 503, "{answer}");
            assert_eq!(answer["error"]["code"], "parental_policy_unavailable");
            let after: Vec<_> = fixture
                .0
                .auth
                .sessions
                .lock()
                .unwrap()
                .iter()
                .map(|session| session.principal.token.clone())
                .collect();
            if before != after {
                changed.push(existing);
            }
        }
        assert!(changed.is_empty(), "failed logins changed bearer sessions with these initial account-session counts: {changed:?}");
    });
}
