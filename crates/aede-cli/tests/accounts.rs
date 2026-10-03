//! Account administration and credential backup through the public CLI.

use aede_core::{accounts, backup, user};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const PASSWORD: &str = "a long test passphrase";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede_accounts_cli_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn run(&self, arguments: &[&str], password: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(arguments)
            .env("AEDE_HOME", &self.0)
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(password) = password {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(password.as_bytes())
                .unwrap();
        }
        drop(child.stdin.take());
        child.wait_with_output().unwrap()
    }

    fn init(&self) {
        let result = self.run(
            &["accounts", "init", "Operator", "--password-stdin"],
            Some(PASSWORD),
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn accounts(&self) -> accounts::Accounts {
        accounts::load(&accounts::accounts_path(&self.0))
            .unwrap()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn initialization_preserves_local_opinions_and_listing_needs_no_catalog() {
    let fixture = Fixture::new();
    user::save(&user::UserData::default(), &user::user_path(&fixture.0)).unwrap();
    let before = std::fs::read(user::user_path(&fixture.0)).unwrap();
    assert!(fixture.run(&["accounts", "--json"], None).status.success());
    fixture.init();
    assert_eq!(
        fixture.accounts().find("operator").unwrap().id,
        user::LOCAL_USER
    );
    assert_eq!(std::fs::read(user::user_path(&fixture.0)).unwrap(), before);
    let result = fixture.run(&["accounts", "list", "--json"], None);
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("operator"));
    assert!(
        !output.contains("password") && !output.contains("argon2") && !output.contains("epoch")
    );
    assert!(
        !fixture
            .run(
                &["accounts", "init", "other", "--password-stdin"],
                Some(PASSWORD)
            )
            .status
            .success()
    );
}

#[test]
fn account_edits_keep_owner_and_last_administrator_and_revoke_credentials() {
    let fixture = Fixture::new();
    fixture.init();
    assert!(
        fixture
            .run(
                &["accounts", "create", "alice", "user", "--password-stdin"],
                Some(PASSWORD)
            )
            .status
            .success()
    );
    assert!(
        fixture
            .run(
                &[
                    "accounts",
                    "create",
                    "auditor",
                    "auditor",
                    "--password-stdin",
                ],
                Some(PASSWORD),
            )
            .status
            .success()
    );
    assert_eq!(
        fixture.accounts().find("auditor").unwrap().role,
        aede_core::accounts::Role::Auditor
    );
    let owner = fixture.accounts().find("alice").unwrap().id.clone();
    let revision = fixture.accounts().find("alice").unwrap().revision();
    for arguments in [
        vec!["accounts", "rename", "alice", "renamed"],
        vec!["accounts", "disable", "renamed"],
        vec!["accounts", "enable", "renamed"],
        vec!["accounts", "revoke", "renamed"],
    ] {
        assert!(fixture.run(&arguments, None).status.success());
    }
    let data = fixture.accounts();
    assert_eq!(data.find("renamed").unwrap().id, owner);
    assert!(data.find("renamed").unwrap().revision() > revision);
    let before = std::fs::read(accounts::accounts_path(&fixture.0)).unwrap();
    assert!(
        !fixture
            .run(&["accounts", "disable", "operator"], None)
            .status
            .success()
    );
    assert!(
        !fixture
            .run(&["accounts", "role", "operator", "user"], None)
            .status
            .success()
    );
    assert_eq!(
        std::fs::read(accounts::accounts_path(&fixture.0)).unwrap(),
        before
    );
    assert!(
        fixture
            .run(
                &["accounts", "password", "renamed", "--password-stdin"],
                Some("another long passphrase\r\n")
            )
            .status
            .success()
    );
    assert!(
        fixture
            .accounts()
            .authenticate("renamed", PASSWORD)
            .is_none()
    );
    assert!(
        fixture
            .accounts()
            .authenticate("renamed", "another long passphrase")
            .is_some()
    );
}

#[test]
fn invalid_arguments_and_passwords_never_publish_credentials() {
    let fixture = Fixture::new();
    for (arguments, password) in [
        (vec!["accounts", "init", "operator"], Some(PASSWORD)),
        (
            vec!["accounts", "init", "operator", "--password-stdin"],
            Some("short"),
        ),
        (
            vec!["accounts", "init", "operator", "--password-stdin"],
            Some("a long test passphrase\nsecond"),
        ),
        (
            vec!["accounts", "init", "../escape", "--password-stdin"],
            Some(PASSWORD),
        ),
        (vec!["accounts", "list", "--password-stdin"], None),
    ] {
        assert!(!fixture.run(&arguments, password).status.success());
        assert!(!accounts::accounts_path(&fixture.0).exists());
    }
}

#[test]
fn account_backup_restores_owners_and_passwords_with_a_fresh_session_epoch() {
    let fixture = Fixture::new();
    fixture.init();
    let issued = fixture.run(
        &["accounts", "keys", "operator", "create", "phone", "--json"],
        None,
    );
    assert!(issued.status.success());
    let issued = aede_core::json::parse(&String::from_utf8(issued.stdout).unwrap()).unwrap();
    let token = issued.get("token").unwrap().as_str().unwrap().to_owned();
    let original = fixture.accounts();
    let archive = fixture.0.join("archive.json");
    assert!(
        fixture
            .run(&["backup", archive.to_str().unwrap()], None)
            .status
            .success()
    );
    assert!(backup::read(&archive).unwrap().accounts.held().is_some());
    assert!(
        fixture
            .run(
                &["accounts", "password", "operator", "--password-stdin"],
                Some("another long passphrase")
            )
            .status
            .success()
    );
    assert!(
        fixture
            .run(&["restore", archive.to_str().unwrap(), "--yes"], None)
            .status
            .success()
    );
    let restored = fixture.accounts();
    assert_eq!(restored.all(), original.all());
    assert_ne!(restored.epoch(), original.epoch());
    assert!(restored.authenticate("operator", PASSWORD).is_some());
    assert!(restored.api_keys("operator").unwrap().is_empty());
    assert!(restored.authenticate_api_key(&token).is_none());
    assert!(
        !fixture
            .run(
                &[
                    "backup",
                    accounts::accounts_path(&fixture.0).to_str().unwrap(),
                    "--yes"
                ],
                None
            )
            .status
            .success()
    );
    assert_eq!(fixture.accounts(), restored);
}

#[test]
fn api_keys_are_shown_once_listed_without_secrets_and_revoked_independently() {
    let fixture = Fixture::new();
    fixture.init();
    let issue = |label: &str| {
        let result = fixture.run(
            &["accounts", "keys", "operator", "create", label, "--json"],
            None,
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        aede_core::json::parse(&String::from_utf8(result.stdout).unwrap()).unwrap()
    };
    let first = issue("Living room");
    let first_token = first.get("token").unwrap().as_str().unwrap();
    let first_id = first.get("id").unwrap().as_str().unwrap();
    assert_eq!(first.get("label").unwrap().as_str(), Some("Living room"));
    assert!(first.get("created_at").unwrap().as_u64().is_some());
    let second = issue("Phone");
    let second_token = second.get("token").unwrap().as_str().unwrap();
    assert!(
        fixture
            .accounts()
            .authenticate_api_key(first_token)
            .is_some()
    );
    assert!(
        fixture
            .accounts()
            .authenticate_api_key(second_token)
            .is_some()
    );
    let listed = fixture.run(&["accounts", "keys", "OPERATOR", "--json"], None);
    assert!(listed.status.success());
    let text = String::from_utf8(listed.stdout).unwrap();
    let rows = aede_core::json::parse(&text).unwrap();
    assert_eq!(rows.as_arr().unwrap().len(), 2);
    assert_eq!(
        rows.as_arr().unwrap()[0].get("id").unwrap().as_str(),
        Some(first_id)
    );
    assert_eq!(
        rows.as_arr().unwrap()[0].get("label").unwrap().as_str(),
        Some("Living room")
    );
    assert!(text.contains("Living room"));
    for private in [
        first_token,
        second_token,
        "token",
        "owner",
        "revision",
        "hash",
        "argon2",
    ] {
        assert!(!text.contains(private), "{private}");
    }
    let stored = std::fs::read_to_string(accounts::accounts_path(&fixture.0)).unwrap();
    assert!(!stored.contains(first_token.split_once('.').unwrap().1));
    assert!(!stored.contains(second_token.split_once('.').unwrap().1));
    let revoked = fixture.run(
        &["accounts", "keys", "operator", "revoke", first_id, "--json"],
        None,
    );
    assert!(revoked.status.success());
    let listed = aede_core::json::parse(&String::from_utf8(revoked.stdout).unwrap()).unwrap();
    assert_eq!(listed.as_arr().unwrap().len(), 1);
    assert!(
        fixture
            .accounts()
            .authenticate_api_key(first_token)
            .is_none()
    );
    assert!(
        fixture
            .accounts()
            .authenticate_api_key(second_token)
            .is_some()
    );
    assert!(
        fixture
            .run(&["accounts", "revoke", "operator"], None)
            .status
            .success()
    );
    assert!(fixture.accounts().api_keys("operator").unwrap().is_empty());
}

#[test]
fn key_reads_remain_available_under_a_writer_lock_and_invalid_commands_show_no_secret() {
    let fixture = Fixture::new();
    fixture.init();
    let before = std::fs::read(accounts::accounts_path(&fixture.0)).unwrap();
    for arguments in [
        vec!["accounts", "keys"],
        vec!["accounts", "keys", "operator", "list"],
        vec!["accounts", "keys", "operator", "create"],
        vec!["accounts", "keys", "operator", "create", " "],
        vec!["accounts", "keys", "operator", "create", "phone", "extra"],
        vec!["accounts", "keys", "operator", "revoke"],
        vec!["accounts", "keys", "operator", "revoke", "unknown"],
        vec!["accounts", "keys", "operator", "unknown", "phone"],
        vec!["accounts", "keys", "operator", "--password-stdin"],
    ] {
        let result = fixture.run(&arguments, None);
        assert!(!result.status.success(), "{arguments:?}");
        assert!(
            result.stdout.is_empty(),
            "no token on refusal: {arguments:?}"
        );
    }
    assert_eq!(
        std::fs::read(accounts::accounts_path(&fixture.0)).unwrap(),
        before
    );
    let held = aede_core::store_lock::StoreLock::acquire(&fixture.0).unwrap();
    assert!(
        fixture
            .run(&["accounts", "keys", "operator", "--json"], None)
            .status
            .success()
    );
    let mut blocked = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args(["accounts", "keys", "operator", "create", "phone", "--json"])
        .env("AEDE_HOME", &fixture.0)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(blocked.try_wait().unwrap().is_none());
    assert_eq!(
        std::fs::read(accounts::accounts_path(&fixture.0)).unwrap(),
        before
    );
    drop(held);
    let published = blocked.wait_with_output().unwrap();
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );
    let published = aede_core::json::parse(&String::from_utf8(published.stdout).unwrap()).unwrap();
    let token = published.get("token").unwrap().as_str().unwrap();
    assert!(fixture.accounts().authenticate_api_key(token).is_some());
}

#[test]
fn legacy_restore_and_reset_preserve_existing_accounts() {
    let fixture = Fixture::new();
    fixture.init();
    assert!(
        fixture
            .run(&["accounts", "keys", "operator", "create", "phone"], None)
            .status
            .success()
    );
    let before = std::fs::read(accounts::accounts_path(&fixture.0)).unwrap();
    let archive = fixture.0.join("legacy.json");
    let value =
        aede_core::json::parse(r#"{"format_version":2,"user":{"format_version":2}}"#).unwrap();
    std::fs::write(&archive, value.to_string_compact()).unwrap();
    // A real readable user store makes this an actionable old-format restore.
    let mut document = value;
    document.set("user", user::to_json(&user::UserData::default()));
    std::fs::write(&archive, document.to_string_compact()).unwrap();
    assert!(
        fixture
            .run(&["restore", archive.to_str().unwrap(), "--yes"], None)
            .status
            .success()
    );
    assert_eq!(
        std::fs::read(accounts::accounts_path(&fixture.0)).unwrap(),
        before
    );
    assert!(fixture.run(&["reset", "--yes"], None).status.success());
    assert_eq!(
        std::fs::read(accounts::accounts_path(&fixture.0)).unwrap(),
        before
    );
}

#[cfg(unix)]
#[test]
fn exposed_credential_destinations_are_refused_before_other_stores_change() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fixture.init();
    let archive = fixture.0.join("archive.json");
    assert!(
        fixture
            .run(&["backup", archive.to_str().unwrap()], None)
            .status
            .success()
    );
    assert_eq!(
        std::fs::metadata(&archive).unwrap().permissions().mode() & 0o077,
        0
    );
    std::fs::set_permissions(&archive, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(backup::read(&archive).is_err());
    assert!(
        !fixture
            .run(&["backup", archive.to_str().unwrap(), "--yes"], None)
            .status
            .success()
    );
    std::fs::set_permissions(&archive, std::fs::Permissions::from_mode(0o600)).unwrap();
    let credentials = accounts::accounts_path(&fixture.0);
    std::fs::set_permissions(&credentials, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        !fixture
            .run(&["restore", archive.to_str().unwrap(), "--yes"], None)
            .status
            .success()
    );
}
