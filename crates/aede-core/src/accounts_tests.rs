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
