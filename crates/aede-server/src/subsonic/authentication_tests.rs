use super::*;
use crate::accounts_test_support::Fixture;

#[test]
fn revocation_during_policy_confirmation_cannot_validate_an_old_client_key() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let fixture = Fixture::new();
        let token = super::super::test_support::key(&fixture, "alice");
        let identity = authenticate(&fixture.0, &Parameters::from_pairs(&[("apiKey", &token)]))
            .await
            .unwrap();
        let state =
            crate::parental::scoped(fixture.0.clone(), identity.owner.clone(), false, false)
                .await
                .unwrap();
        let (release, worker) = crate::test_support::hold_blocking_worker().await;
        let mut confirmation = Box::pin(recheck(&state, &identity));
        std::future::poll_fn(|context| {
            assert!(confirmation.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(release);
        worker.await.unwrap();
        tokio::task::spawn_blocking(|| ()).await.unwrap();
        let (release, worker) = crate::test_support::hold_blocking_worker().await;
        std::future::poll_fn(|context| {
            assert!(confirmation.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        let mut accounts = fixture.accounts();
        accounts.revoke_api_key("alice", &identity.key_id).unwrap();
        aede_core::accounts::save(
            &accounts,
            &aede_core::accounts::accounts_path(&state.data_dir),
        )
        .unwrap();
        drop(release);
        worker.await.unwrap();
        let result = confirmation.await;
        assert!(
            result.is_err(),
            "a policy result cannot validate a key revoked while it was queued"
        );
        assert!(!aede_core::user::user_path(&state.data_dir).exists());
    });
}

#[test]
fn policy_publication_during_final_key_check_refuses_the_result() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let fixture = Fixture::new();
        let token = super::super::test_support::key(&fixture, "alice");
        let identity = authenticate(&fixture.0, &Parameters::from_pairs(&[("apiKey", &token)]))
            .await
            .unwrap();
        let state =
            crate::parental::scoped(fixture.0.clone(), identity.owner.clone(), false, false)
                .await
                .unwrap();
        let mut confirmation = Box::pin(recheck(&state, &identity));
        for _ in 0..2 {
            let (release, worker) = crate::test_support::hold_blocking_worker().await;
            std::future::poll_fn(|context| {
                assert!(confirmation.as_mut().poll(context).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            drop(release);
            worker.await.unwrap();
            tokio::task::spawn_blocking(|| ()).await.unwrap();
        }
        let (release, worker) = crate::test_support::hold_blocking_worker().await;
        std::future::poll_fn(|context| {
            assert!(confirmation.as_mut().poll(context).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        state.parental.invalidate();
        drop(release);
        worker.await.unwrap();
        assert!(
            confirmation.await.is_err(),
            "the policy publication changed during the final key read"
        );
        assert!(!aede_core::user::user_path(&state.data_dir).exists());
    });
}

#[test]
fn a_revocable_key_in_the_legacy_password_field_authenticates_only_its_named_owner() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let token = super::super::test_support::key(&fixture, "alice");
        let parameters = Parameters::from_pairs(&[("u", "ALICE"), ("p", &token)]);
        let identity = authenticate(&fixture.0, &parameters).await.unwrap();
        assert_eq!(identity.username, "alice");
        let hex = token
            .bytes()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let encoded = format!("enc:{hex}");
        let parameters = Parameters::from_pairs(&[("u", "alice"), ("p", &encoded)]);
        assert_eq!(
            authenticate(&fixture.0, &parameters).await.unwrap().owner,
            identity.owner
        );
        let parameters = Parameters::from_pairs(&[("u", "operator"), ("p", &token)]);
        assert_eq!(
            authenticate(&fixture.0, &parameters)
                .await
                .unwrap_err()
                .code,
            44
        );
        let mut accounts = fixture.accounts();
        accounts
            .revoke_api_key("alice", token.split_once('.').unwrap().0)
            .unwrap();
        aede_core::accounts::save(
            &accounts,
            &aede_core::accounts::accounts_path(&fixture.0.data_dir),
        )
        .unwrap();
        let parameters = Parameters::from_pairs(&[("u", "alice"), ("p", &token)]);
        assert_eq!(
            authenticate(&fixture.0, &parameters)
                .await
                .unwrap_err()
                .code,
            44
        );
    });
}

#[test]
fn authentication_never_combines_keys_passwords_or_md5_credentials() {
    for fields in [
        vec![("apiKey", "key"), ("u", "alice")],
        vec![("apiKey", "key"), ("p", "secret")],
        vec![("u", "alice"), ("p", "secret"), ("t", "md5"), ("s", "salt")],
    ] {
        assert_eq!(
            credential(&Parameters::from_pairs(&fields))
                .unwrap_err()
                .code,
            43
        );
    }
    assert_eq!(
        credential(&Parameters::from_pairs(&[
            ("u", "alice"),
            ("t", "md5"),
            ("s", "salt")
        ]))
        .unwrap_err()
        .code,
        41
    );
    assert_eq!(
        credential(&Parameters::from_pairs(&[("p", "x")]))
            .unwrap_err()
            .code,
        10
    );
    for encoded in ["enc:", "enc:xx", "enc:é"] {
        assert_eq!(
            credential(&Parameters::from_pairs(&[("u", "alice"), ("p", encoded)]))
                .unwrap_err()
                .code,
            10
        );
    }
}
