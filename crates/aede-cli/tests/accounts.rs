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
fn legacy_restore_and_reset_preserve_existing_accounts() {
    let fixture = Fixture::new();
    fixture.init();
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
