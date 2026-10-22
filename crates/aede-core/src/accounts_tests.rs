use super::*;

use super::test_support::*;

#[test]
fn historical_credential_documents_keep_numeric_spelling_limits_and_private_key_records() {
    let mut data = accounts();
    let (key, token) = data.create_api_key("operator", "desktop", 20).unwrap();
    let document = to_json(&data);
    let historical = document
        .to_string_compact()
        .replace("\"format_version\":1", "\"format_version\":1.0")
        .replace("\"revision\":1", "\"revision\":1e0")
        .replace("\"created_at\":10", "\"created_at\":-0")
        .replace("\"updated_at\":10", "\"updated_at\":9.007199254740991e15")
        .replace("\"created_at\":20", "\"created_at\":2e1");
    let loaded = from_json(&json::parse(&historical).unwrap()).unwrap();
    let account = loaded.authenticate("operator", PASSWORD).unwrap();
    assert_eq!(account.id, LOCAL_OWNER);
    assert_eq!(account.created_at, 0);
    assert_eq!(account.updated_at, (1 << 53) - 1);
    assert_eq!(account.revision(), 1);
    assert_eq!(loaded.authenticate_api_key(&token).unwrap().1.id, key.id);
    assert_eq!(loaded.api_keys("operator").unwrap()[0].created_at, 20);
    let rewritten = to_json(&loaded);
    assert_eq!(from_json(&rewritten).unwrap(), loaded);
    assert!(!rewritten.to_string_compact().contains(&token));
    assert!(rewritten.get("api_keys").is_some());
    assert!(to_json(&accounts()).get("api_keys").is_none());

    for revision in [0.0, 1.5, (1_u64 << 53) as f64, f64::INFINITY] {
        let mut changed = document.clone();
        let mut row = changed.get("account").unwrap().as_arr().unwrap()[0].clone();
        row.set("revision", Json::Num(revision));
        changed.set("account", Json::Arr(vec![row]));
        assert!(matches!(
            from_json(&changed),
            Err(StoreError::AccountsInvalid(
                "malformed account store or unsupported version"
            ))
        ));
    }
    let mut invalid_key = document;
    invalid_key.set("api_keys", Json::Null);
    assert!(matches!(
        from_json(&invalid_key),
        Err(StoreError::AccountsInvalid(
            "malformed or excessive API key records"
        ))
    ));
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

#[test]
fn excessive_argon2_lanes_in_account_files_are_refused_without_panicking_or_writing() {
    let dir = std::env::temp_dir().join(format!("aede_accounts_lanes_{}", random_token().unwrap()));
    let path = accounts_path(&dir);
    let mut data = accounts();
    data.create("listener", PASSWORD, Role::User, 11).unwrap();
    data.create_api_key("operator", "desktop", 20).unwrap();
    let (_, token) = data.create_api_key("listener", "player", 21).unwrap();
    save(&data, &path).unwrap();
    let unrelated = crate::user::user_path(&dir);
    crate::user::save(&crate::user::UserData::default(), &unrelated).unwrap();
    let unrelated_bytes = std::fs::read(&unrelated).unwrap();
    let document = to_json(&data);
    let mut outcomes = Vec::new();

    for (table, field) in [("account", "password_hash"), ("api_keys", "secret_hash")] {
        let mut corrupt = document.clone();
        let mut rows = corrupt.get(table).unwrap().as_arr().unwrap().to_vec();
        let hash = rows.last().unwrap().get(field).unwrap().as_str().unwrap();
        let hash = hash.replace("p=1$", "p=4294967295$");
        rows.last_mut().unwrap().set(field, hash.into());
        corrupt.set(table, Json::Arr(rows));
        let corrupt_bytes = corrupt.to_string_compact().into_bytes();
        std::fs::write(&path, &corrupt_bytes).unwrap();

        let result = std::panic::catch_unwind(|| load(&path));
        outcomes.push((
            table,
            matches!(result, Ok(Err(StoreError::AccountsInvalid(_)))),
        ));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt_bytes);
        assert_eq!(std::fs::read(&unrelated).unwrap(), unrelated_bytes);
    }

    save(&data, &path).unwrap();
    let loaded = load(&path).unwrap().unwrap();
    assert_eq!(loaded, data);
    assert!(loaded.authenticate("listener", PASSWORD).is_some());
    assert!(loaded.authenticate_api_key(&token).is_some());
    std::fs::remove_dir_all(dir).unwrap();
    assert!(outcomes.iter().all(|(_, refused)| *refused), "{outcomes:?}");
}

#[test]
fn undecodable_salts_in_account_files_are_refused_without_writing() {
    let dir = std::env::temp_dir().join(format!("aede_accounts_salts_{}", random_token().unwrap()));
    let path = accounts_path(&dir);
    let mut data = accounts();
    data.create("listener", PASSWORD, Role::User, 11).unwrap();
    data.create_api_key("operator", "desktop", 20).unwrap();
    let (_, token) = data.create_api_key("listener", "player", 21).unwrap();
    save(&data, &path).unwrap();
    let unrelated = crate::user::user_path(&dir);
    crate::user::save(&crate::user::UserData::default(), &unrelated).unwrap();
    let unrelated_bytes = std::fs::read(&unrelated).unwrap();
    let document = to_json(&data);
    let mut outcomes = Vec::new();

    for (table, field) in [("account", "password_hash"), ("api_keys", "secret_hash")] {
        for salt in [
            "YWFhYWFhYWFhYWFhYWFhY.",
            "YWFhYWFhYWFhYWFhYWFhY-",
            "YWFhYWFhYWFhYWFhYWFhYR",
        ] {
            let mut corrupt = document.clone();
            let mut rows = corrupt.get(table).unwrap().as_arr().unwrap().to_vec();
            let hash = rows.last().unwrap().get(field).unwrap().as_str().unwrap();
            let mut parts = hash.split('$').collect::<Vec<_>>();
            parts[4] = salt;
            let hash = parts.join("$");
            rows.last_mut().unwrap().set(field, hash.into());
            corrupt.set(table, Json::Arr(rows));
            let corrupt_bytes = corrupt.to_string_compact().into_bytes();
            std::fs::write(&path, &corrupt_bytes).unwrap();

            let result = std::panic::catch_unwind(|| load(&path));
            outcomes.push((
                table,
                salt,
                matches!(result, Ok(Err(StoreError::AccountsInvalid(_)))),
            ));
            assert_eq!(std::fs::read(&path).unwrap(), corrupt_bytes);
            assert_eq!(std::fs::read(&unrelated).unwrap(), unrelated_bytes);
        }
    }

    save(&data, &path).unwrap();
    let loaded = load(&path).unwrap().unwrap();
    assert_eq!(loaded, data);
    assert!(loaded.authenticate("listener", PASSWORD).is_some());
    assert!(loaded.authenticate_api_key(&token).is_some());
    std::fs::remove_dir_all(dir).unwrap();
    assert!(
        outcomes.iter().all(|(_, _, refused)| *refused),
        "{outcomes:?}"
    );
}

#[test]
fn corrupt_backup_credentials_leave_personal_data_recoverable_and_source_bytes_intact() {
    use crate::backup::{Backup, Part};

    let dir =
        std::env::temp_dir().join(format!("aede_accounts_backup_{}", random_token().unwrap()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("backup.json");
    let mut data = accounts();
    data.create_api_key("operator", "desktop", 20).unwrap();
    let user = crate::user::UserData {
        set_aside: vec![crate::user::SetAside {
            owner: LOCAL_OWNER.into(),
            release_group: "saved-release".into(),
            title: "A saved release".into(),
            created_at: 12,
        }],
        ..Default::default()
    };
    let backup = Backup {
        made_at: 30,
        made_by: "test".into(),
        catalog: Part::Empty,
        conclusions: Part::Empty,
        user: Part::Held(user),
        sources: Part::Empty,
        accounts: Part::Held(data.clone()),
        parental: Part::Empty,
    };
    crate::backup::write(&backup, &path).unwrap();
    assert_eq!(
        crate::backup::read(&path).unwrap().accounts.held(),
        Some(&data)
    );
    let document = crate::backup::to_json(&backup);
    let user_document = document.get("user").unwrap().clone();
    let mut outcomes = Vec::new();

    for (table, field, hash) in [
        (
            "account",
            "password_hash",
            "$argon2id$v=19$m=19456,t=2,p=4294967295$YWFhYWFhYWFhYWFhYWFhYQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ),
        (
            "api_keys",
            "secret_hash",
            "$argon2id$v=19$m=19456,t=2,p=1$YWFhYWFhYWFhYWFhYWFhYR$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ),
    ] {
        let mut corrupt = document.clone();
        let mut credentials = corrupt.get("accounts").unwrap().clone();
        let mut rows = credentials.get(table).unwrap().as_arr().unwrap().to_vec();
        rows[0].set(field, hash.into());
        credentials.set(table, Json::Arr(rows));
        corrupt.set("accounts", credentials);
        let corrupt_bytes = corrupt.to_string_pretty().into_bytes();
        std::fs::write(&path, &corrupt_bytes).unwrap();

        let result = std::panic::catch_unwind(|| crate::backup::read(&path));
        let recoverable = match result {
            Ok(Ok(restored)) => {
                assert_eq!(
                    crate::user::to_json(restored.user.held().unwrap()),
                    user_document
                );
                matches!(restored.accounts, Part::Unreadable(_))
            }
            _ => false,
        };
        outcomes.push((table, recoverable));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt_bytes);
    }

    std::fs::remove_dir_all(dir).unwrap();
    assert!(outcomes.iter().all(|(_, refused)| *refused), "{outcomes:?}");
}

#[test]
fn rejected_account_timestamps_keep_saved_credentials_and_client_keys() {
    let dir = std::env::temp_dir().join(format!("aede_accounts_time_{}", random_token().unwrap()));
    let path = accounts_path(&dir);
    let mut data = accounts();
    let (_, token) = data.create_api_key("operator", "desktop", 20).unwrap();
    save(&data, &path).unwrap();
    let saved_bytes = std::fs::read(&path).unwrap();
    let unrelated = crate::user::user_path(&dir);
    crate::user::save(&crate::user::UserData::default(), &unrelated).unwrap();
    let unrelated_bytes = std::fs::read(&unrelated).unwrap();

    let mut loaded = load(&path).unwrap().unwrap();
    assert!(loaded.revoke("operator", 1 << 53).is_err());
    assert_eq!(loaded, data);
    save(&loaded, &path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
    assert_eq!(std::fs::read(&unrelated).unwrap(), unrelated_bytes);
    let reloaded = load(&path).unwrap().unwrap();
    assert_eq!(reloaded, data);
    assert!(reloaded.authenticate("operator", PASSWORD).is_some());
    assert!(reloaded.authenticate_api_key(&token).is_some());
    std::fs::remove_dir_all(dir).unwrap();
}
