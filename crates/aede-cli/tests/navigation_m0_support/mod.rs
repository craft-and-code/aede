//! Isolated local graph fixture for browsing and export regressions.

use aede_core::{model, store, tags};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Library {
    pub dir: PathBuf,
    pub catalog: model::Catalog,
}

impl Library {
    pub fn new(genre: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aede-navigation-m0-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut scanned = Vec::new();
        for album in 1..=3 {
            for track in 1..=2 {
                let mut tags = tags::RawTags::default();
                tags.insert("album", format!("Needle Album {album}"));
                tags.insert("title", format!("Needle {album}{track}"));
                tags.insert("artist", "Lead");
                tags.insert("albumartist", "Lead");
                tags.insert("performer", "Guest");
                tags.insert("genre", genre);
                tags.insert("genre", format!("Facet {album}"));
                tags.insert("label", format!("Label {album}"));
                tags.insert("comment", "needle in comments");
                tags.insert("tracknumber", track.to_string());
                tags.insert("date", format!("200{album}"));
                scanned.push(model::ScannedFile {
                    path: format!("/music/Needle Album {album}/0{track}.flac"),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                });
            }
        }
        let catalog = model::build(scanned, vec!["/music".into()], 1, &[]);
        store::save(&catalog, &store::catalog_path(&dir)).unwrap();
        Self { dir, catalog }
    }

    pub fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aede"))
            .args(args)
            .arg("--data")
            .arg(&self.dir)
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    pub fn success(&self, args: &[&str]) -> String {
        let result = self.run(args);
        assert!(
            result.status.success(),
            "{args:?}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
