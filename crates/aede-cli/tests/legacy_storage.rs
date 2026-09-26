//! Legacy conclusions remain available without a read-side migration write.

use std::path::PathBuf;
use std::process::Command;

use aede_core::{backup, conclusions, json, store};

struct LegacyLibrary {
    root: PathBuf,
    data: PathBuf,
}

impl LegacyLibrary {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aede_legacy_storage_{}_{nonce}",
            std::process::id()
        ));
        let music = root.join("music");
        std::fs::create_dir_all(&music).unwrap();
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../aede-core/tests/fixtures/track.flac"),
            music.join("track.flac"),
        )
        .unwrap();
        let library = Self {
            data: root.join("data"),
            root,
        };
        library.run(&["scan", music.to_str().unwrap()]);
        let path = store::catalog_path(&library.data);
        let mut legacy = json::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let mut files = legacy.get("file").unwrap().as_arr().unwrap().to_vec();
        let mut integrity = json::Json::obj();
        integrity.set("state", "intact".into());
        integrity.set("method", "flac-frame-crc".into());
        integrity.set("checked_at", 1_700_000_500u64.into());
        files[0].set("integrity", integrity);
        let mut fingerprint = json::Json::obj();
        fingerprint.set("data", "legacy-fingerprint".into());
        fingerprint.set("seconds", 120u32.into());
        files[0].set("fingerprint", fingerprint);
        legacy.set("file", json::Json::Arr(files));
        let mut analysis = json::Json::obj();
        analysis.set("path", "/music/not-yet-scanned.flac".into());
        analysis.set("source", "flaccompagnon".into());
        legacy.set("analysis", json::Json::Arr(vec![analysis]));
        std::fs::write(path, legacy.to_string_compact()).unwrap();
        assert!(!conclusions::conclusions_path(&library.data).exists());
        library
    }

    fn run(&self, args: &[&str]) {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(args)
            .arg("--data")
            .arg(&self.data)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

impl Drop for LegacyLibrary {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn read_backup_and_graph_export_preserve_legacy_conclusions_without_migrating_the_source() {
    let library = LegacyLibrary::new();
    let path = store::catalog_path(&library.data);
    let original = std::fs::read(&path).unwrap();
    library.run(&["stats"]);
    assert!(!conclusions::conclusions_path(&library.data).exists());

    let bundle = library.root.join("backup.json");
    library.run(&["backup", bundle.to_str().unwrap()]);
    let saved = backup::read(&bundle).unwrap();
    let saved_conclusions = saved.conclusions.held().unwrap();
    assert_eq!(saved_conclusions.files.len(), 1);
    assert_eq!(saved_conclusions.analyses.len(), 1);
    let graph = library.root.join("graph.json");
    library.run(&["export", "--graph", "--output", graph.to_str().unwrap()]);
    let exported = json::parse(&std::fs::read_to_string(graph).unwrap()).unwrap();
    let gathered = conclusions::from_json(exported.get("conclusions").unwrap()).unwrap();
    assert_eq!(gathered.files.len(), 1);
    assert_eq!(gathered.analyses.len(), 1);
    assert_eq!(
        gathered
            .files
            .values()
            .next()
            .unwrap()
            .fingerprint
            .as_ref()
            .unwrap()
            .data,
        "legacy-fingerprint"
    );
    assert!(!conclusions::conclusions_path(&library.data).exists());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn a_first_full_scan_migrates_legacy_conclusions_without_losing_them() {
    let library = LegacyLibrary::new();
    library.run(&["scan", "--full"]);
    let catalog = store::load(&store::catalog_path(&library.data))
        .unwrap()
        .unwrap();
    assert_eq!(
        catalog.files[0].integrity.as_ref().unwrap().checked_at,
        1_700_000_500
    );
    assert!(conclusions::conclusions_path(&library.data).exists());
    assert_eq!(
        catalog.files[0].fingerprint.as_ref().unwrap().data,
        "legacy-fingerprint"
    );
    assert_eq!(catalog.analyses.len(), 1);
}
