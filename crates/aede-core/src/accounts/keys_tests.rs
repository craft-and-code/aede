use super::*;

use super::super::test_support::*;

#[test]
fn issued_keys_round_trip_without_persisting_or_debugging_the_secret() {
    let mut data = accounts();
    assert!(data.api_keys("operator").unwrap().is_empty());
    assert!(super::super::from_json(&super::super::to_json(&data)).is_ok());
    let (key, token) = data
        .create_api_key("OPERATOR", " Living room ", 20)
        .unwrap();
    assert_eq!(token.len(), 129);
    let (id, secret) = token.split_once('.').unwrap();
    assert_eq!(id, key.id);
    assert!(valid_token(id) && valid_token(secret));
    assert_eq!(key.label, " Living room ");
    assert_eq!(key.created_at, 20);
    let (account, authenticated) = data.authenticate_api_key(&token).unwrap();
    assert_eq!(account.id, crate::user::LOCAL_USER);
    assert_eq!(authenticated.id, key.id);
    assert!(data.authenticate_api_key(PASSWORD).is_none());
    assert!(data.authenticate("operator", &token).is_none());

    let document = super::super::to_json(&data);
    let text = document.to_string_compact();
    assert!(!text.contains(secret) && !text.contains(&token));
    let debug = format!("{data:?}");
    assert!(!debug.contains("argon2") && !debug.contains(secret));
    let loaded = super::super::from_json(&document).unwrap();
    assert_eq!(loaded, data);
    assert_eq!(loaded.authenticate_api_key(&token).unwrap().1.id, key.id);
    assert_eq!(loaded.api_keys("operator").unwrap().len(), 1);
}

#[test]
fn single_key_revocation_is_owner_scoped_and_invalidates_cached_authentication() {
    let mut data = accounts();
    data.create("alice", PASSWORD, Role::User, 20).unwrap();
    let (first, first_token) = data.create_api_key("alice", "phone", 21).unwrap();
    let (second, second_token) = data.create_api_key("alice", "tablet", 22).unwrap();
    let (_, operator_token) = data.create_api_key("operator", "desktop", 23).unwrap();
    let owner = data.find("alice").unwrap().id.clone();
    let revision = data.find("alice").unwrap().revision();
    let epoch = data.epoch().to_owned();
    assert!(
        data.api_key_account(&first.id, &owner, &epoch, revision)
            .is_some()
    );
    for (id, owner, epoch, revision) in [
        (first.id.as_str(), "local", epoch.as_str(), revision),
        (first.id.as_str(), owner.as_str(), "wrong epoch", revision),
        (
            first.id.as_str(),
            owner.as_str(),
            epoch.as_str(),
            revision + 1,
        ),
        ("missing", owner.as_str(), epoch.as_str(), revision),
    ] {
        assert!(data.api_key_account(id, owner, epoch, revision).is_none());
    }
    let before = data.clone();
    assert!(data.revoke_api_key("operator", &first.id).is_err());
    assert_eq!(data, before);
    assert!(
        data.authenticate_api_key(&format!("{}.{}", first.id, "0".repeat(64)))
            .is_none()
    );
    assert!(
        data.authenticate_api_key(&format!("{}.{}", "f".repeat(64), "0".repeat(64)))
            .is_none()
    );
    for malformed in [
        format!("{first_token}.extra"),
        first_token.to_ascii_uppercase(),
        first.id.clone(),
    ] {
        assert!(data.authenticate_api_key(&malformed).is_none());
    }
    data.revoke_api_key("ALICE", &first.id).unwrap();
    assert!(data.authenticate_api_key(&first_token).is_none());
    assert!(
        data.api_key_account(&first.id, &owner, &epoch, revision)
            .is_none()
    );
    assert!(data.session_account(&owner, &epoch, revision).is_some());
    assert!(data.authenticate_api_key(&second_token).is_some());
    assert!(data.authenticate_api_key(&operator_token).is_some());
    assert_eq!(data.api_keys("alice").unwrap()[0].id, second.id);
}

#[test]
fn account_changes_and_epoch_rotation_purge_keys_without_weakening_failed_edits() {
    let mut base = accounts();
    base.create("alice", PASSWORD, Role::User, 20).unwrap();
    let (key, token) = base.create_api_key("alice", "phone", 21).unwrap();
    let (_, operator_token) = base.create_api_key("operator", "desktop", 22).unwrap();
    let owner = base.find("alice").unwrap().id.clone();
    let epoch = base.epoch().to_owned();
    let revision = base.find("alice").unwrap().revision();
    for change in 0..5 {
        let mut data = base.clone();
        match change {
            0 => data.rename("alice", "listener", 30).unwrap(),
            1 => data.set_role("alice", Role::Auditor, 30).unwrap(),
            2 => data.set_enabled("alice", false, 30).unwrap(),
            3 => data
                .set_password("alice", "another long passphrase", 30)
                .unwrap(),
            _ => data.revoke("alice", 30).unwrap(),
        }
        assert!(data.authenticate_api_key(&token).is_none());
        assert!(
            data.api_key_account(&key.id, &owner, &epoch, revision)
                .is_none()
        );
        assert!(data.authenticate_api_key(&operator_token).is_some());
        assert!(super::super::from_json(&super::super::to_json(&data)).is_ok());
    }
    let mut unchanged = base.clone();
    assert!(unchanged.set_enabled("operator", false, 30).is_err());
    unchanged.set_role("alice", Role::User, 30).unwrap();
    assert_eq!(unchanged, base);
    let mut restored = super::super::from_json(&super::super::to_json(&base)).unwrap();
    restored.revoke_all().unwrap();
    assert!(restored.api_keys("alice").unwrap().is_empty());
    assert!(restored.api_keys("operator").unwrap().is_empty());
    assert!(restored.authenticate_api_key(&token).is_none());
    assert_ne!(restored.epoch(), epoch);
}

#[test]
fn malformed_key_records_are_refused_as_a_whole_credential_store() {
    let mut data = accounts();
    data.create_api_key("operator", "phone", 20).unwrap();
    let base = super::super::to_json(&data);
    let row = base.get("api_keys").unwrap().as_arr().unwrap()[0].clone();
    for (field, value) in [
        ("id", "bad".into()),
        ("label", "\u{2003}".into()),
        ("label", "é".repeat(65).into()),
        ("owner", "missing-owner".into()),
        ("revision", 0_u64.into()),
        ("revision", 2_u64.into()),
        ("created_at", 1.5.into()),
        (
            "secret_hash",
            DUMMY_HASH.replace("m=19456", "m=4294967295").into(),
        ),
        ("secret_hash", "not a verifier".into()),
    ] {
        let mut changed = row.clone();
        changed.set(field, value);
        let mut document = base.clone();
        document.set("api_keys", Json::Arr(vec![changed]));
        assert!(super::super::from_json(&document).is_err(), "{field}");
    }
    let mut duplicate = base.clone();
    duplicate.set("api_keys", Json::Arr(vec![row.clone(), row]));
    assert!(super::super::from_json(&duplicate).is_err());
    for value in [Json::Null, "not an array".into()] {
        let mut document = base.clone();
        document.set("api_keys", value);
        assert!(super::super::from_json(&document).is_err());
    }
    let mut missing = base.clone();
    missing.set("api_keys", Json::Arr(Vec::new()));
    assert!(
        super::super::from_json(&missing)
            .unwrap()
            .api_keys("operator")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn labels_use_utf8_byte_limits_and_disabled_accounts_cannot_receive_keys() {
    let mut data = accounts();
    data.create("alice", PASSWORD, Role::User, 20).unwrap();
    data.set_enabled("alice", false, 21).unwrap();
    let before = data.clone();
    for label in ["".to_string(), " \t\n ".into(), "é".repeat(65)] {
        assert!(data.create_api_key("operator", &label, 22).is_err());
    }
    assert!(data.create_api_key("missing", "phone", 22).is_err());
    assert!(data.create_api_key("alice", "phone", 22).is_err());
    assert!(
        data.create_api_key("operator", "phone", MAX_REVISION + 1)
            .is_err()
    );
    assert_eq!(data, before);
    let (key, _) = data
        .create_api_key("operator", &"é".repeat(64), 22)
        .unwrap();
    assert_eq!(key.label.len(), MAX_API_KEY_LABEL_BYTES);
}

#[test]
fn key_counts_are_bounded_during_creation_and_store_parsing() {
    let mut data = accounts();
    data.create_api_key("operator", "phone", 20).unwrap();
    let base = super::super::to_json(&data);
    let original_key = base.get("api_keys").unwrap().as_arr().unwrap()[0].clone();
    let original_account = base.get("account").unwrap().as_arr().unwrap()[0].clone();
    let mut rows = Vec::new();
    for index in 0..MAX_API_KEYS_PER_ACCOUNT {
        let mut key = original_key.clone();
        key.set("id", format!("{index:064x}").into());
        rows.push(key);
    }
    let mut document = base.clone();
    document.set("api_keys", Json::Arr(rows.clone()));
    let mut full_account = super::super::from_json(&document).unwrap();
    let before = full_account.clone();
    assert!(
        full_account
            .create_api_key("operator", "ninth", 21)
            .is_err()
    );
    assert_eq!(full_account, before);
    let mut excess = original_key.clone();
    excess.set("id", format!("{:064x}", MAX_API_KEYS_PER_ACCOUNT).into());
    rows.push(excess);
    document.set("api_keys", Json::Arr(rows));
    assert!(super::super::from_json(&document).is_err());

    let mut accounts = vec![original_account.clone()];
    let mut keys = Vec::new();
    for owner_index in 0..MAX_API_KEYS / MAX_API_KEYS_PER_ACCOUNT {
        let owner = if owner_index == 0 {
            "local".to_string()
        } else {
            let owner = format!("account-{owner_index:064x}");
            let mut account = original_account.clone();
            account.set("id", owner.clone().into());
            account.set("username", format!("user{owner_index}").into());
            account.set("role", "user".into());
            accounts.push(account);
            owner
        };
        for key_index in 0..MAX_API_KEYS_PER_ACCOUNT {
            let mut key = original_key.clone();
            key.set("id", format!("{:064x}", keys.len()).into());
            key.set("owner", owner.clone().into());
            key.set("label", format!("client{key_index}").into());
            keys.push(key);
        }
    }
    document = base;
    document.set("account", Json::Arr(accounts));
    document.set("api_keys", Json::Arr(keys.clone()));
    let mut full_installation = super::super::from_json(&document).unwrap();
    assert!(
        full_installation
            .create_api_key("operator", "overflow", 21)
            .is_err()
    );
    assert_eq!(
        full_installation,
        super::super::from_json(&document).unwrap()
    );
    keys.push(original_key);
    document.set("api_keys", Json::Arr(keys));
    assert!(super::super::from_json(&document).is_err());
}
