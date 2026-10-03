use super::*;
use crate::accounts_test_support::Fixture;

#[test]
fn now_playing_is_private_short_lived_and_never_claims_a_listening_event() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let alice_token = super::super::test_support::key(&fixture, "alice");
        let bob_token = super::super::test_support::key(&fixture, "bob");
        let alice = authentication::authenticate(
            &fixture.0,
            &Parameters::from_pairs(&[("apiKey", &alice_token)]),
        )
        .await
        .unwrap();
        let bob = authentication::authenticate(
            &fixture.0,
            &Parameters::from_pairs(&[("apiKey", &bob_token)]),
        )
        .await
        .unwrap();
        let song = opaque_id(
            fixture.0.catalog.read().await.as_ref().unwrap(),
            EntityKind::Track,
            0,
        )
        .unwrap();
        let at = aede_core::clock::now_seconds()
            .saturating_mul(1000)
            .saturating_sub(120_000)
            .to_string();
        let parameters = Parameters::from_pairs(&[
            ("id", &song),
            ("c", "Desktop"),
            ("time", &at),
            ("submission", "false"),
        ]);
        report(&fixture.0, &alice, &parameters).await.unwrap();
        let value = list(&fixture.0, &alice, &Parameters::default())
            .await
            .unwrap();
        assert_eq!(value["nowPlaying"]["entry"][0]["username"], "alice");
        assert_eq!(value["nowPlaying"]["entry"][0]["playerName"], "Desktop");
        assert_eq!(value["nowPlaying"]["entry"][0]["minutesAgo"], 2);
        assert_eq!(
            list(&fixture.0, &bob, &Parameters::default())
                .await
                .unwrap()["nowPlaying"]["entry"],
            json!([])
        );
        assert!(!aede_core::user::user_path(&fixture.0.data_dir).exists());
        fixture.0.subsonic.now_playing.lock().unwrap()[0].expires = Instant::now();
        assert_eq!(
            list(&fixture.0, &alice, &Parameters::default())
                .await
                .unwrap()["nowPlaying"]["entry"],
            json!([])
        );
        report(&fixture.0, &alice, &parameters).await.unwrap();
        let mut accounts = fixture.accounts();
        accounts.revoke_api_key("alice", &alice.key_id).unwrap();
        aede_core::accounts::save(
            &accounts,
            &aede_core::accounts::accounts_path(&fixture.0.data_dir),
        )
        .unwrap();
        assert_eq!(
            list(&fixture.0, &alice, &Parameters::default())
                .await
                .unwrap()["nowPlaying"]["entry"],
            json!([])
        );
        assert!(!aede_core::user::user_path(&fixture.0.data_dir).exists());
    });
}

#[test]
fn now_playing_refuses_auditors_unknown_songs_and_ambiguous_reports() {
    crate::test_support::test_runtime().block_on(async {
        let fixture = Fixture::new();
        let token = super::super::test_support::key(&fixture, "auditor");
        let auditor = authentication::authenticate(
            &fixture.0,
            &Parameters::from_pairs(&[("apiKey", &token)]),
        )
        .await
        .unwrap();
        assert_eq!(
            report(
                &fixture.0,
                &auditor,
                &Parameters::from_pairs(&[("id", "song"), ("c", "test")])
            )
            .await
            .unwrap_err()
            .code,
            50
        );
        let token = super::super::test_support::key(&fixture, "alice");
        let alice = authentication::authenticate(
            &fixture.0,
            &Parameters::from_pairs(&[("apiKey", &token)]),
        )
        .await
        .unwrap();
        for fields in [
            vec![("id", "missing"), ("c", "test")],
            vec![("id", "a"), ("id", "b"), ("c", "test")],
            vec![("id", "a"), ("time", "-1"), ("c", "test")],
        ] {
            assert!(
                report(&fixture.0, &alice, &Parameters::from_pairs(&fields))
                    .await
                    .is_err()
            );
        }
        assert!(fixture.0.subsonic.now_playing.lock().unwrap().is_empty());
    });
}
