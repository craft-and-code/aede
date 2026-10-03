use super::*;

use super::test_support::*;

#[test]
fn bootstrap_preserves_local_ownership_and_passwords_are_salted_and_private() {
    let first = accounts();
    let second = accounts();
    assert_eq!(first.all()[0].id, crate::user::LOCAL_USER);
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
    assert!(!to_json(&first).to_string_compact().contains(PASSWORD));
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
        ("revision", 1.5.into()),
        (
            "password_hash",
            DUMMY_HASH.replace("m=19456", "m=4294967295").into(),
        ),
        ("password_hash", DUMMY_HASH.replace("m=19456", "m=8").into()),
    ] {
        let mut changed = base.clone();
        let mut row = changed.get("account").unwrap().as_arr().unwrap()[0].clone();
        row.set(key, value);
        changed.set("account", Json::Arr(vec![row]));
        variants.push(changed);
    }
    let mut duplicate = base.clone();
    let row = duplicate.get("account").unwrap().as_arr().unwrap()[0].clone();
    duplicate.set("account", Json::Arr(vec![row.clone(), row]));
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
fn a_private_store_round_trips_without_writing_through_a_link() {
    let dir = std::env::temp_dir().join(format!("aede_accounts_{}", random_token().unwrap()));
    let path = accounts_path(&dir);
    let data = accounts();
    assert!(load(&path).unwrap().is_none());
    save(&data, &path).unwrap();
    assert_eq!(load(&path).unwrap().unwrap(), data);
    std::fs::write(&path, vec![b' '; MAX_FILE_BYTES as usize + 1]).unwrap();
    assert!(load(&path).unwrap_err().to_string().contains("1 MiB"));
    save(&data, &path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
        let linked = dir.join("linked.json");
        symlink(&path, &linked).unwrap();
        assert!(load(&linked).is_err());
        assert!(save(&data, &linked).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&path).is_err());
        assert!(save(&data, &path).is_err());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn credential_reads_refuse_a_replacement_or_permission_change_during_open() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let dir = std::env::temp_dir().join(format!("aede_accounts_open_{}", random_token().unwrap()));
    let path = accounts_path(&dir);
    let replacement = dir.join("replacement.json");
    let data = accounts();
    save(&data, &path).unwrap();
    save(&data, &replacement).unwrap();
    let result = load_with_open(&path, |path| {
        std::fs::remove_file(path)?;
        symlink(&replacement, path)?;
        std::fs::File::open(path)
    });
    assert!(
        result.is_err(),
        "a replaced source must not supply credentials"
    );

    std::fs::remove_file(&path).unwrap();
    save(&data, &path).unwrap();
    let result = load_with_open(&path, |path| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))?;
        std::fs::File::open(path)
    });
    assert!(
        result.is_err(),
        "the opened credentials must still be private"
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let result = load_with_open(&path, |path| {
        std::fs::remove_file(path)?;
        std::fs::File::open(path)
    });
    assert!(
        result.is_err(),
        "observed credentials disappearing must not look like initial absence"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
