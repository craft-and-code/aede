use super::*;
use crate::accounts_test_support::{Fixture, http, login};
use crate::test_support::{start_server, test_runtime};

#[test]
fn a_pending_catalog_read_cannot_publish_after_logout_or_revocation_for_any_role() {
    test_runtime().block_on(async {
        let mut published = Vec::new();
        for name in ["operator", "alice", "auditor"] {
            for revoke in [false, true] {
                let fixture = Fixture::new();
                let (address, server) = start_server(fixture.0.clone()).await;
                let token = login(address, name);
                let catalog = fixture.0.catalog.write().await;
                let slots = fixture.0.inspection_slots.available_permits();
                let for_read = token.clone();
                let response = tokio::task::spawn_blocking(move || {
                    http(address, "GET", "/api/v1/artists", Some(&for_read), "")
                });
                wait_for_catalog_read(&fixture.0, slots).await;
                if revoke {
                    let _guard = StoreLock::try_acquire(&fixture.0.data_dir).unwrap();
                    let mut accounts = fixture.accounts();
                    accounts.revoke(name, 20).unwrap();
                    aede_core::accounts::save(
                        &accounts,
                        &aede_core::accounts::accounts_path(&fixture.0.data_dir),
                    )
                    .unwrap();
                } else {
                    assert_eq!(
                        http(address, "DELETE", "/api/auth/v1/session", Some(&token), "").0,
                        204
                    );
                }
                drop(catalog);
                let (status, _, _) = response.await.unwrap();
                server.abort();
                if status != 401 {
                    published.push((name, revoke, status));
                }
            }
        }
        assert!(
            published.is_empty(),
            "retired logins published pending catalogue reads: {published:?}"
        );
    });
}

#[test]
fn an_anonymous_catalog_read_waiting_during_account_activation_cannot_publish() {
    test_runtime().block_on(async {
        let fixture = Fixture::new();
        let accounts = fixture.accounts();
        let path = aede_core::accounts::accounts_path(&fixture.0.data_dir);
        std::fs::remove_file(&path).unwrap();
        let (address, server) = start_server(fixture.0.clone()).await;
        let catalog = fixture.0.catalog.write().await;
        let slots = fixture.0.inspection_slots.available_permits();
        let response =
            tokio::task::spawn_blocking(move || http(address, "GET", "/api/v1/artists", None, ""));
        wait_for_catalog_read(&fixture.0, slots).await;
        aede_core::accounts::save(&accounts, &path).unwrap();
        let _ = login(address, "operator");
        drop(catalog);
        let (status, _, _) = response.await.unwrap();
        server.abort();
        assert_eq!(
            status, 401,
            "activation must retire pending anonymous catalogue reads"
        );
    });
}

async fn wait_for_catalog_read(state: &ApiState, slots: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.inspection_slots.available_permits() == slots {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the admitted read must be waiting for the catalogue");
}
