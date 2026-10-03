use super::*;
use crate::accounts_test_support::Fixture;

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
