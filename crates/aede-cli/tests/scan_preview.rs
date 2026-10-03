//! Scan previews report changes without publishing any of the independent stores.

use std::path::{Path, PathBuf};
use std::process::Command;

use aede_core::json::{self, Json};

struct Sandbox(PathBuf);

impl Sandbox {
    fn new() -> Self {
        static NEXT_SANDBOX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT_SANDBOX.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aede_scan_preview_{}_{nonce}_{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }

    fn fixture(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
            &path,
        )
        .unwrap();
        path
    }

    fn run(&self, data: &Path, extra: &[&str]) -> Json {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .arg("--data")
            .arg(data)
            .arg("scan")
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        json::parse(&String::from_utf8(output.stdout).unwrap()).unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn dry_run_reports_changes_without_changing_any_store_or_watched_root() {
    let sandbox = Sandbox::new();
    let changed = sandbox.fixture("music/changed.flac");
    let removed = sandbox.fixture("music/gone.flac");
    let added = sandbox.fixture("another/new.flac");
    let data = sandbox.0.join("data");
    let second = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let set_modified = |fraction| {
        std::fs::File::options()
            .write(true)
            .open(&changed)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(second + std::time::Duration::from_nanos(fraction)),
            )
            .unwrap();
    };
    set_modified(100_000_000);
    let original = sandbox.run(
        &data,
        &[sandbox.0.join("music").to_str().unwrap(), "--json"],
    );
    assert_eq!(original.get("dry_run"), Some(&Json::Bool(false)));
    let user_path = aede_core::user::user_path(&data);
    aede_core::user::save(&Default::default(), &user_path).unwrap();
    aede_core::conclusions::save(
        &Default::default(),
        &aede_core::conclusions::conclusions_path(&data),
    )
    .unwrap();
    let sources = data.join("sources.json");
    std::fs::write(&sources, b"preserve independent source data").unwrap();
    let files = [
        aede_core::store::catalog_path(&data),
        aede_core::conclusions::conclusions_path(&data),
        user_path,
        sources,
    ];
    let before: Vec<_> = files
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect();
    set_modified(800_000_000);
    std::fs::remove_file(&removed).unwrap();
    let preview = sandbox.run(
        &data,
        &[
            sandbox.0.join("another").to_str().unwrap(),
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(preview.get("dry_run"), Some(&Json::Bool(true)));
    for (name, expected) in [("added", added), ("changed", changed), ("removed", removed)] {
        assert_eq!(
            preview.get(name),
            Some(&Json::Arr(vec![expected.to_string_lossy().as_ref().into()])),
            "{name}"
        );
    }
    for (path, bytes) in files.iter().zip(before) {
        assert_eq!(
            std::fs::read(path).unwrap(),
            bytes,
            "{} was mutated by a preview",
            path.display()
        );
    }
    let stored = aede_core::store::load(&files[0]).unwrap().unwrap();
    assert_eq!(
        stored.roots.len(),
        1,
        "preview must not watch the proposed new root"
    );
}

#[test]
fn first_scan_preview_does_not_create_the_data_directory() {
    let sandbox = Sandbox::new();
    sandbox.fixture("music/01.flac");
    let data = sandbox.0.join("not-created");
    let preview = sandbox.run(
        &data,
        &[
            sandbox.0.join("music").to_str().unwrap(),
            "--dry-run",
            "--json",
        ],
    );
    assert_eq!(preview.get("counts").unwrap().field_u64("added"), Some(1));
    assert!(!data.exists());
}

#[test]
fn an_unavailable_watched_root_does_not_block_other_folders_or_drop_its_tracks() {
    let sandbox = Sandbox::new();
    let absent = sandbox.fixture("offline/01.flac");
    let accessible = sandbox.fixture("online/01.flac");
    let data = sandbox.0.join("data");
    sandbox.run(
        &data,
        &[
            sandbox.0.join("offline").to_str().unwrap(),
            sandbox.0.join("online").to_str().unwrap(),
            "--json",
        ],
    );
    std::fs::remove_dir_all(absent.parent().unwrap()).unwrap();
    let added = sandbox.fixture("online/02.flac");
    let preview = sandbox.run(&data, &["--dry-run", "--json"]);
    assert_eq!(
        preview.get("counts").unwrap().field_u64("preserved"),
        Some(1)
    );
    assert_eq!(
        preview.get("counts").unwrap().field_u64("catalogued"),
        Some(3)
    );
    assert_eq!(
        preview.get("added"),
        Some(&Json::Arr(vec![added.to_string_lossy().as_ref().into()]))
    );
    assert!(preview.get("removed").unwrap().as_arr().unwrap().is_empty());
    let published = sandbox.run(&data, &["--json"]);
    assert_eq!(
        published.get("counts").unwrap().field_u64("preserved"),
        Some(1)
    );
    let catalog = aede_core::store::load(&aede_core::store::catalog_path(&data))
        .unwrap()
        .unwrap();
    for path in [absent, accessible, added] {
        assert!(
            catalog
                .files
                .iter()
                .any(|file| Path::new(&file.path) == path)
        );
    }
}

#[test]
fn scan_captures_legacy_note_identity_before_publishing_a_moved_track() {
    let sandbox = Sandbox::new();
    let original = sandbox.fixture("music/Old/01.flac");
    let data = sandbox.0.join("data");
    sandbox.run(
        &data,
        &[sandbox.0.join("music").to_str().unwrap(), "--json"],
    );
    let before = aede_core::store::load(&aede_core::store::catalog_path(&data))
        .unwrap()
        .unwrap();
    let old_reference =
        aede_core::user::EntityRef::of(&before, aede_core::model::EntityKind::Track, 0).unwrap();
    let mut personal = aede_core::user::UserData::default();
    personal
        .entry(aede_core::user::LOCAL_USER, &old_reference, 1)
        .note = Some("keep this note".into());
    assert!(
        personal.track_identities.is_empty(),
        "this represents legacy data without identity evidence"
    );
    aede_core::user::save(&personal, &aede_core::user::user_path(&data)).unwrap();
    let moved = sandbox.0.join("music/New/01.flac");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    sandbox.run(&data, &["--json"]);
    let after = aede_core::store::load(&aede_core::store::catalog_path(&data))
        .unwrap()
        .unwrap();
    let personal = aede_core::user::load(&aede_core::user::user_path(&data))
        .unwrap()
        .unwrap();
    let target =
        aede_core::user::EntityRef::of(&after, aede_core::model::EntityKind::Track, 0).unwrap();
    assert_eq!(
        personal
            .find(aede_core::user::LOCAL_USER, &target)
            .unwrap()
            .note
            .as_deref(),
        Some("keep this note")
    );
    assert_eq!(Path::new(&target.key), moved);
}

#[test]
fn scan_keeps_a_note_waiting_when_a_same_named_file_has_different_bytes() {
    let sandbox = Sandbox::new();
    let original = sandbox.fixture("music/Old/01.flac");
    let data = sandbox.0.join("data");
    sandbox.run(
        &data,
        &[sandbox.0.join("music").to_str().unwrap(), "--json"],
    );
    let before = aede_core::store::load(&aede_core::store::catalog_path(&data))
        .unwrap()
        .unwrap();
    let old_reference =
        aede_core::user::EntityRef::of(&before, aede_core::model::EntityKind::Track, 0).unwrap();
    let mut personal = aede_core::user::UserData::default();
    personal
        .entry(aede_core::user::LOCAL_USER, &old_reference, 1)
        .note = Some("original bytes".into());
    aede_core::user::save(&personal, &aede_core::user::user_path(&data)).unwrap();
    let replacement = sandbox.fixture("music/Other/01.flac");
    let mut bytes = std::fs::read(&replacement).unwrap();
    bytes.push(0);
    std::fs::write(&replacement, bytes).unwrap();
    std::fs::remove_file(&original).unwrap();
    sandbox.run(&data, &["--json"]);
    let after = aede_core::store::load(&aede_core::store::catalog_path(&data))
        .unwrap()
        .unwrap();
    let personal = aede_core::user::load(&aede_core::user::user_path(&data))
        .unwrap()
        .unwrap();
    assert_eq!(personal.annotations[0].target, old_reference);
    assert!(old_reference.resolve(&after).is_none());
}
