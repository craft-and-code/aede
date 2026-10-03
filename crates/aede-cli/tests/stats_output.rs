//! Statistics page their subsidiary lists and include every persistent store.

use aede_core::model::{self, ScannedFile};
use aede_core::{conclusions, json, store, tags, text};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Library(PathBuf);

impl Library {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "aede-stats-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let files = ["Alpha", "Beta", "Gamma"]
            .into_iter()
            .enumerate()
            .map(|(i, artist)| {
                let mut tags = tags::RawTags::default();
                tags.insert("artist", artist);
                tags.insert("albumartist", artist);
                tags.insert("album", artist);
                tags.insert("title", artist);
                tags.insert("producer", artist);
                if i == 0 {
                    tags.insert("composer", artist);
                }
                tags.properties.codec = ["flac", "mp3", "wav"][i].into();
                ScannedFile {
                    path: format!("/music/{artist}/track.flac"),
                    size: 1,
                    mtime: 1,
                    tags,
                    folder_cover: None,
                    sidecar: None,
                    integrity: None,
                    fingerprint: None,
                }
            })
            .collect();
        let catalog = model::build(files, vec!["/music".into()], 1, &[]);
        store::save(&catalog, &store::catalog_path(&dir)).unwrap();
        Self(dir)
    }

    fn run(&self, extra: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_aede"))
            .arg("stats")
            .arg("--data")
            .arg(&self.0)
            .args(extra)
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn pagination_applies_to_human_and_json_lists_without_changing_totals() {
    let library = Library::new();
    let out = library.run(&["--limit=1", "--offset=1", "--json"]);
    let report = json::parse(&out).unwrap();
    assert_eq!(report.get("tracks").and_then(|v| v.as_u64()), Some(3));
    let artists = report
        .get("top")
        .unwrap()
        .get("artists")
        .unwrap()
        .as_arr()
        .unwrap();
    assert_eq!(artists.len(), 1);
    assert_eq!(
        artists[0].get("name").and_then(|v| v.as_str()),
        Some("Beta")
    );
    let formats = report.get("by_codec").unwrap().as_arr().unwrap();
    assert_eq!(formats.len(), 1);
    assert_eq!(
        formats[0].get("label").and_then(|v| v.as_str()),
        Some("MP3")
    );
    let roles = report.get("roles").unwrap().as_arr().unwrap();
    assert_eq!(roles.len(), 1);
    assert_eq!(
        roles[0].get("role").and_then(|v| v.as_str()),
        Some("composer")
    );

    let human = library.run(&["--limit=1", "--offset=1"]);
    let ranking = human.split("Most present performers").nth(1).unwrap();
    assert!(ranking.contains("Beta"), "{human}");
    assert!(
        !ranking.contains("Alpha") && !ranking.contains("Gamma"),
        "{human}"
    );

    let all = json::parse(&library.run(&["--all", "--json"])).unwrap();
    assert_eq!(
        all.get("top")
            .unwrap()
            .get("artists")
            .unwrap()
            .as_arr()
            .unwrap()
            .len(),
        3
    );
    let beyond = json::parse(&library.run(&["--offset=100", "--json"])).unwrap();
    assert!(
        beyond
            .get("top")
            .unwrap()
            .get("artists")
            .unwrap()
            .as_arr()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn data_weight_includes_the_conclusions_store() {
    let library = Library::new();
    let mut saved = conclusions::to_json(&conclusions::Conclusions::default());
    // Unknown optional fields are valid; make this file visibly outweigh the catalog.
    saved.set("future_evidence", "x".repeat(20_000).into());
    let path = conclusions::conclusions_path(&library.0);
    std::fs::write(&path, saved.to_string_pretty()).unwrap();
    let bytes = std::fs::metadata(store::catalog_path(&library.0))
        .unwrap()
        .len()
        + std::fs::metadata(path).unwrap().len();
    let output = library.run(&[]);
    assert!(output.contains(&text::format_size(bytes)), "{output}");
}
