use super::*;
use crate::accounts_test_support::{Fixture, PASSWORD};
use crate::test_support::hold_blocking_worker as hold_worker;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

#[test]
fn session_metadata_queued_before_logout_or_revocation_cannot_publish_after_it() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        for revoke in [false, true] {
            let fixture = Fixture::new();
            let mut accounts = fixture.accounts();
            let captured =
                auth::start_session(&fixture.0, &accounts, accounts.find("alice").unwrap())
                    .unwrap()
                    .0;
            let mut request = Request::builder()
                .uri("/api/auth/v1/session")
                .body(axum::body::Body::empty())
                .unwrap();
            request.extensions_mut().insert(captured.clone());
            let mut response = Box::pin(session_info(State(fixture.0.clone()), request));

            // Make both scheduling boundaries deterministic: finish the credential
            // read, then hold the separate metadata worker until access is retired.
            let (release, worker) = hold_worker().await;
            pending_once(response.as_mut()).await;
            drop(release);
            worker.await.unwrap();
            tokio::task::spawn_blocking(|| ()).await.unwrap();
            let (release, worker) = hold_worker().await;
            pending_once(response.as_mut()).await;
            assert_eq!(fixture.0.parental.read_slots.available_permits(), 7);
            if revoke {
                accounts.revoke("alice", 20).unwrap();
                accounts::save(&accounts, &accounts::accounts_path(&fixture.0.data_dir)).unwrap();
            } else {
                auth::end_session(&fixture.0, &captured).unwrap();
            }
            drop(release);
            worker.await.unwrap();
            let failure = response
                .await
                .err()
                .expect("retired session metadata must be refused");
            assert_eq!(failure.status, StatusCode::UNAUTHORIZED);
        }
    });
}

async fn pending_once<F: Future>(mut future: Pin<&mut F>) {
    poll_fn(|context| {
        assert!(
            future.as_mut().poll(context).is_pending(),
            "the worker must still be queued"
        );
        Poll::Ready(())
    })
    .await;
}

#[test]
fn logout_during_account_mutation_preparation_preserves_the_credential_store() {
    crate::test_support::test_runtime().block_on(async {
        for create in [true, false] {
            let fixture = Fixture::new();
            let data = fixture.accounts();
            let name = if create { "operator" } else { "alice" };
            let captured = auth::start_session(&fixture.0, &data, data.find(name).unwrap())
                .unwrap()
                .0;
            let path = accounts::accounts_path(&fixture.0.data_dir);
            let original = std::fs::read(&path).unwrap();
            let result = password_update(
                fixture.0.clone(),
                Some(captured),
                move |state, actor, data| {
                    if create {
                        data.create("new-listener", PASSWORD, Role::User, 20)
                            .map_err(refused)?;
                    } else {
                        data.set_password("alice", "a different long test passphrase", 20)
                            .map_err(refused)?;
                    }
                    // Logout does not take the credential writer lock. It can retire
                    // this login while password hashing prepares an in-memory change.
                    auth::end_session(state, actor.unwrap())?;
                    Ok(())
                },
            )
            .await;
            assert!(
                std::fs::read(&path).unwrap() == original,
                "a retired account mutation must not replace credentials (create={create})"
            );
            assert_eq!(
                result
                    .expect_err("logout must refuse the prepared account mutation")
                    .status,
                StatusCode::UNAUTHORIZED
            );
        }
    });
}
