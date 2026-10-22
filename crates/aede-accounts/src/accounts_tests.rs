use super::*;

use super::test_support::*;

#[test]
fn bootstrap_preserves_local_ownership_and_passwords_are_salted_and_private() {
    let first = accounts();
    let second = accounts();
    assert_eq!(first.all()[0].id, LOCAL_OWNER);
    assert_eq!(first.all()[0].username, "operator");
    assert_ne!(
        first.records[0].password_hash,
        second.records[0].password_hash
    );
    assert!(first.authenticate("OPERATOR", PASSWORD).is_some());
    for (name, password) in [
        ("operator", "incorrect"),
        ("missing", PASSWORD),
        ("../escape", PASSWORD),
    ] {
        assert!(first.authenticate(name, password).is_none());
    }
    assert!(!format!("{first:?}").contains("argon2"));
    assert!(!to_json(&first).to_string().contains(PASSWORD));
}

#[test]
fn creation_and_renaming_preserve_distinct_owners_and_refuse_duplicates() {
    let mut data = accounts();
    data.create("Alice", PASSWORD, Role::User, 20).unwrap();
    let id = data.find("alice").unwrap().id.clone();
    assert_ne!(id, "local");
    assert!(data.create("ALICE", PASSWORD, Role::User, 21).is_err());
    assert!(data.rename("alice", "operator", 21).is_err());
    data.rename("alice", "listener", 22).unwrap();
    assert_eq!(data.find("listener").unwrap().id, id);
    assert!(data.find("alice").is_none());
}

#[test]
fn auditor_role_round_trips_with_its_read_only_spelling() {
    let mut data = accounts();
    data.create("auditor", PASSWORD, Role::Auditor, 20).unwrap();

    assert_eq!(Role::Auditor.as_str(), "auditor");
    assert_eq!(Role::parse("auditor"), Some(Role::Auditor));
    assert_eq!(Role::parse("AUDITOR"), None);
    assert_eq!(
        from_json(&to_json(&data))
            .unwrap()
            .find("auditor")
            .unwrap()
            .role,
        Role::Auditor
    );
}

#[test]
fn disabling_and_demoting_the_last_administrator_leave_the_store_unchanged() {
    let mut data = accounts();
    let before = data.clone();
    assert!(data.set_enabled("operator", false, 20).is_err());
    assert!(data.set_role("operator", Role::User, 20).is_err());
    assert_eq!(data, before);
    data.create("second", PASSWORD, Role::Administrator, 20)
        .unwrap();
    data.set_enabled("operator", false, 21).unwrap();
    assert!(data.authenticate("operator", PASSWORD).is_none());
    assert!(data.authenticate("second", PASSWORD).is_some());
    assert!(data.set_enabled("second", false, 22).is_err());
}

#[test]
fn password_and_role_changes_and_restoration_revoke_old_session_generations() {
    let mut data = accounts();
    data.create("alice", PASSWORD, Role::User, 20).unwrap();
    let owner = data.find("alice").unwrap().id.clone();
    let epoch = data.epoch().to_string();
    let revision = data.find("alice").unwrap().revision();
    assert!(data.session_account(&owner, &epoch, revision).is_some());
    data.set_password("alice", "a different long passphrase", 21)
        .unwrap();
    assert!(data.session_account(&owner, &epoch, revision).is_none());
    assert!(data.authenticate("alice", PASSWORD).is_none());
    let revision = data.find("alice").unwrap().revision();
    data.set_role("alice", Role::Auditor, 22).unwrap();
    assert!(data.session_account(&owner, &epoch, revision).is_none());
    let revision = data.find("alice").unwrap().revision();
    data.revoke_all().unwrap();
    assert!(data.session_account(&owner, &epoch, revision).is_none());
    assert!(
        data.authenticate("alice", "a different long passphrase")
            .is_some()
    );
}

#[test]
fn malformed_or_hostile_rows_never_become_a_partial_or_weaker_account_store() {
    let base = to_json(&accounts());
    let mut variants = Vec::new();
    for (key, value) in [
        ("id", "unexpected-owner".into()),
        ("username", "../escape".into()),
        ("role", "user".into()),
        ("enabled", false.into()),
        ("revision", serde_json::json!(1.5)),
        (
            "password_hash",
            DUMMY_HASH.replace("m=19456", "m=4294967295").into(),
        ),
        ("password_hash", DUMMY_HASH.replace("m=19456", "m=8").into()),
    ] {
        let mut changed = base.clone();
        let mut row = changed.get("account").unwrap().as_array().unwrap()[0].clone();
        row[key] = value;
        changed["account"] = Json::Array(vec![row]);
        variants.push(changed);
    }
    let mut duplicate = base.clone();
    let row = duplicate.get("account").unwrap().as_array().unwrap()[0].clone();
    duplicate["account"] = Json::Array(vec![row.clone(), row]);
    variants.push(duplicate);
    for value in variants {
        assert!(from_json(&value).is_err(), "{value:?}");
    }
    assert_eq!(from_json(&base).unwrap().all().len(), 1);
}

#[test]
fn invalid_names_passwords_and_revisions_do_not_change_accounts() {
    let mut data = accounts();
    let before = data.clone();
    for name in ["", "a b", "a/b", "é", "---"] {
        assert!(data.create(name, PASSWORD, Role::User, 20).is_err());
    }
    for password in ["short".to_string(), "x".repeat(MAX_PASSWORD_BYTES + 1)] {
        assert!(data.set_password("operator", &password, 20).is_err());
    }
    assert_eq!(data, before);
    data.records[0].revision = MAX_REVISION;
    assert!(data.revoke("operator", 21).is_err());
}

#[test]
fn bootstrap_refuses_timestamps_outside_the_persisted_range() {
    let accepted: Vec<_> = [MAX_REVISION + 1, u64::MAX]
        .into_iter()
        .filter(|&now| Accounts::bootstrap("operator", PASSWORD, now).is_ok())
        .collect();
    assert!(accepted.is_empty(), "accepted timestamps: {accepted:?}");
}

#[test]
fn creation_refuses_unpersistable_timestamps_without_changing_credentials() {
    let mut base = accounts();
    let (key, _) = base.create_api_key("operator", "desktop", 20).unwrap();
    let epoch = base.epoch().to_owned();
    let revision = base.find("operator").unwrap().revision();
    let mut violations = Vec::new();
    for now in [MAX_REVISION + 1, u64::MAX] {
        let mut data = base.clone();
        let refused = data.create("alice", PASSWORD, Role::User, now).is_err();
        let unchanged = data == base
            && data
                .session_account(LOCAL_OWNER, &epoch, revision)
                .is_some()
            && data
                .api_key_account(&key.id, LOCAL_OWNER, &epoch, revision)
                .is_some();
        if !refused || !unchanged {
            violations.push((now, refused, unchanged));
        }
    }
    assert!(violations.is_empty(), "invalid creations: {violations:?}");
}

#[test]
fn administration_refuses_unpersistable_timestamps_without_changing_credentials() {
    let mut base = accounts();
    base.create("alice", PASSWORD, Role::User, 20).unwrap();
    let (key, _) = base.create_api_key("alice", "phone", 21).unwrap();
    let (operator_key, _) = base.create_api_key("operator", "desktop", 22).unwrap();
    let owner = base.find("alice").unwrap().id.clone();
    let epoch = base.epoch().to_owned();
    let revision = base.find("alice").unwrap().revision();
    let operator_revision = base.find("operator").unwrap().revision();
    let mut violations = Vec::new();
    for now in [MAX_REVISION + 1, u64::MAX] {
        for change in ["role", "enabled", "username", "password", "revoke"] {
            let mut data = base.clone();
            let refused = match change {
                "role" => data.set_role("alice", Role::Auditor, now),
                "enabled" => data.set_enabled("alice", false, now),
                "username" => data.rename("alice", "listener", now),
                "password" => data.set_password("alice", "another long passphrase", now),
                _ => data.revoke("alice", now),
            }
            .is_err();
            let unchanged = data == base
                && data.session_account(&owner, &epoch, revision).is_some()
                && data
                    .session_account(LOCAL_OWNER, &epoch, operator_revision)
                    .is_some()
                && data
                    .api_key_account(&key.id, &owner, &epoch, revision)
                    .is_some()
                && data
                    .api_key_account(&operator_key.id, LOCAL_OWNER, &epoch, operator_revision)
                    .is_some();
            if !refused || !unchanged {
                violations.push((change, now, refused, unchanged));
            }
        }
    }
    assert!(violations.is_empty(), "invalid edits: {violations:?}");
}

#[test]
fn maximum_persistable_timestamp_round_trips_after_creation_and_administration() {
    let mut base = Accounts::bootstrap("operator", PASSWORD, MAX_REVISION).unwrap();
    base.create("alice", PASSWORD, Role::User, MAX_REVISION)
        .unwrap();
    let (key, _) = base.create_api_key("alice", "phone", MAX_REVISION).unwrap();
    let (operator_key, _) = base
        .create_api_key("operator", "desktop", MAX_REVISION)
        .unwrap();
    assert_eq!(base.all()[0].created_at, MAX_REVISION);
    assert_eq!(base.find("alice").unwrap().created_at, MAX_REVISION);
    assert_eq!(key.created_at, MAX_REVISION);
    assert_eq!(from_json(&to_json(&base)).unwrap(), base);

    let owner = base.find("alice").unwrap().id.clone();
    let epoch = base.epoch().to_owned();
    let revision = base.find("alice").unwrap().revision();
    let operator_revision = base.find("operator").unwrap().revision();
    for change in ["role", "enabled", "username", "password", "revoke"] {
        let mut data = base.clone();
        match change {
            "role" => data.set_role("alice", Role::Auditor, MAX_REVISION),
            "enabled" => data.set_enabled("alice", false, MAX_REVISION),
            "username" => data.rename("alice", "listener", MAX_REVISION),
            "password" => data.set_password("alice", "another long passphrase", MAX_REVISION),
            _ => data.revoke("alice", MAX_REVISION),
        }
        .unwrap();
        let account = data
            .all()
            .iter()
            .find(|account| account.id == owner)
            .unwrap();
        assert_eq!(account.updated_at, MAX_REVISION, "{change}");
        assert_eq!(account.revision(), revision + 1, "{change}");
        assert!(data.session_account(&owner, &epoch, revision).is_none());
        assert!(data.api_keys(&account.username).unwrap().is_empty());
        assert!(
            data.api_key_account(&operator_key.id, LOCAL_OWNER, &epoch, operator_revision)
                .is_some()
        );
        assert_eq!(from_json(&to_json(&data)).unwrap(), data, "{change}");
    }
}

#[test]
fn administration_no_ops_keep_credentials_unchanged_with_unused_timestamps() {
    let mut data = accounts();
    let (key, _) = data.create_api_key("operator", "desktop", 20).unwrap();
    let before = data.clone();
    let epoch = data.epoch().to_owned();
    let revision = data.find("operator").unwrap().revision();
    for now in [MAX_REVISION + 1, u64::MAX] {
        data.set_role("operator", Role::Administrator, now).unwrap();
        data.set_enabled("operator", true, now).unwrap();
        data.rename("operator", "OPERATOR", now).unwrap();
        assert_eq!(data, before);
        assert!(
            data.session_account(LOCAL_OWNER, &epoch, revision)
                .is_some()
        );
        assert!(
            data.api_key_account(&key.id, LOCAL_OWNER, &epoch, revision)
                .is_some()
        );
    }
}
