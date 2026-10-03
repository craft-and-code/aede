//! Backup and restore regressions through the public commands.

use std::path::{Path, PathBuf};
use std::process::Command;

use aede_core::backup::{self, Backup, Part};
use aede_core::{conclusions, model, sources, store, user};

struct Fixture {
    directory: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "aede_backup_safety_{name}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        let data = directory.join("data");
        std::fs::create_dir(&data).unwrap();
        user::save(&user::UserData::default(), &user::user_path(&data)).unwrap();
        Self { directory, data }
    }

    fn run(&self, command: &str, path: &Path) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_aede"))
            .arg(command)
            .arg(path)
            .arg("--yes")
            .env("AEDE_HOME", &self.data)
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    fn bundle(&self, path: &Path) {
        backup::write(
            &Backup {
                made_at: 42,
                made_by: "audit".into(),
                catalog: Part::Held(model::build(
                    Vec::new(),
                    vec!["restored-root".into()],
                    42,
                    &[],
                )),
                conclusions: Part::Held(conclusions::Conclusions::default()),
                user: Part::Held(user::UserData::default()),
                sources: Part::Held(sources::Sources::default()),
            },
            path,
        )
        .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[test]
fn a_backup_cannot_replace_an_active_store_or_library_audio() {
    let fixture = Fixture::new("protected_output");
    let path = user::user_path(&fixture.data);
    let original = std::fs::read(&path).unwrap();
    let result = fixture.run("backup", &path);
    assert!(
        !result.status.success(),
        "a backup must preserve active personal data"
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);

    let audio = fixture.directory.join("original.flac");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        &audio,
    )
    .unwrap();
    let original = std::fs::read(&audio).unwrap();
    let result = fixture.run("backup", &audio);
    assert!(!result.status.success());
    assert!(std::fs::read(audio).unwrap() == original);
}

#[test]
fn restore_refuses_invalid_later_destinations_before_replacing_any_store() {
    let fixture = Fixture::new("restore_preflight");
    let catalog_path = store::catalog_path(&fixture.data);
    store::save_catalog_only(
        &model::build(Vec::new(), vec!["current-root".into()], 1, &[]),
        &catalog_path,
    )
    .unwrap();
    let before = std::fs::read(&catalog_path).unwrap();
    let bundle = fixture.directory.join("archive.json");
    fixture.bundle(&bundle);
    std::fs::create_dir(sources::sources_path(&fixture.data)).unwrap();
    let result = fixture.run("restore", &bundle);
    assert!(!result.status.success());
    assert_eq!(
        std::fs::read(catalog_path).unwrap(),
        before,
        "no earlier store may be replaced before known-invalid destinations are rejected"
    );
    assert!(!conclusions::conclusions_path(&fixture.data).exists());
}

#[test]
fn restore_preserves_the_input_archive_when_it_names_a_store_destination() {
    let fixture = Fixture::new("restore_input_alias");
    let bundle = store::catalog_path(&fixture.data);
    fixture.bundle(&bundle);
    let before = std::fs::read(&bundle).unwrap();
    let result = fixture.run("restore", &bundle);
    assert!(
        !result.status.success(),
        "restoring must not overwrite its source archive"
    );
    assert_eq!(std::fs::read(bundle).unwrap(), before);
}

#[test]
fn restore_preserves_its_archive_when_case_aliases_name_a_store_destination() {
    let fixture = Fixture::new("restore_case_alias");
    let bundle = store::catalog_path(&fixture.data);
    fixture.bundle(&bundle);
    let before = std::fs::read(&bundle).unwrap();
    let alias = fixture.data.join("CATALOG.JSON");
    if alias.exists() {
        let result = fixture.run("restore", &alias);
        assert!(
            !result.status.success(),
            "filesystem aliases of the input archive must be refused"
        );
        assert_eq!(std::fs::read(&bundle).unwrap(), before);
    } else {
        // On a case-sensitive filesystem this spelling is a missing file.
        let result = fixture.run("restore", &alias);
        assert!(!result.status.success());
        assert_eq!(std::fs::read(&bundle).unwrap(), before);
    }
}

#[test]
fn malformed_archives_leave_current_stores_unchanged() {
    let fixture = Fixture::new("corrupt_archive");
    let current = user::user_path(&fixture.data);
    let before = std::fs::read(&current).unwrap();
    let archive = fixture.directory.join("archive.json");
    for text in ["{", "not json", "{\"format_version\":999}"] {
        std::fs::write(&archive, text).unwrap();
        let result = fixture.run("restore", &archive);
        assert!(!result.status.success());
        assert_eq!(std::fs::read(&current).unwrap(), before);
        assert!(!store::catalog_path(&fixture.data).exists());
        assert!(!conclusions::conclusions_path(&fixture.data).exists());
        assert!(!sources::sources_path(&fixture.data).exists());
    }
}

#[cfg(unix)]
#[test]
fn restore_refuses_special_files_without_waiting_for_input() {
    let fixture = Fixture::new("special_archive");
    let archive = fixture.directory.join("archive.json");
    assert!(
        Command::new("mkfifo")
            .arg(&archive)
            .status()
            .unwrap()
            .success()
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_aede"))
        .arg("restore")
        .arg(&archive)
        .arg("--yes")
        .env("AEDE_HOME", &fixture.data)
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let timed_out = loop {
        if child.try_wait().unwrap().is_some() {
            break false;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            break true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let output = child.wait_with_output().unwrap();
    assert!(!timed_out, "restore must refuse a FIFO before opening it");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ordinary file"));
}

#[test]
fn readable_parts_restore_while_an_unsupported_part_and_its_destination_remain_untouched() {
    let fixture = Fixture::new("partial_archive");
    let archive = fixture.directory.join("archive.json");
    fixture.bundle(&archive);
    let mut document = aede_core::json::parse(&std::fs::read_to_string(&archive).unwrap()).unwrap();
    let mut source_document = document.get("sources").unwrap().clone();
    source_document.set("format_version", 999u32.into());
    document.set("sources", source_document);
    std::fs::write(&archive, document.to_string_pretty()).unwrap();
    let original = std::fs::read(&archive).unwrap();
    // Skipped parts are never preflighted for writing or deleted.
    let source_folder = sources::sources_path(&fixture.data);
    std::fs::create_dir(&source_folder).unwrap();
    std::fs::write(source_folder.join("sentinel"), b"retain this").unwrap();
    let result = fixture.run("restore", &archive);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("3 stores restored"));
    assert_eq!(
        std::fs::read(source_folder.join("sentinel")).unwrap(),
        b"retain this"
    );
    assert!(store::catalog_path(&fixture.data).is_file());
    assert!(conclusions::conclusions_path(&fixture.data).is_file());
    assert_eq!(std::fs::read(archive).unwrap(), original);
}
