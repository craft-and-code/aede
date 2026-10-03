//! Output safety must be checked before a command mutates personal data.

use aede_core::model::{self, EntityKind, ScannedFile};
use aede_core::user::{self, EntityRef, LOCAL_USER, UserData};
use aede_core::{json, store, tags};
use std::path::PathBuf;
use std::process::Command;

struct OutputDirectory(PathBuf);

impl Drop for OutputDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn track_json_is_written_to_the_requested_file_with_its_rich_fields() {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = OutputDirectory(std::env::temp_dir().join(format!(
        "aede-track-json-output-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    let mut tags = tags::RawTags::default();
    tags.insert("title", "Only song");
    tags.insert("artist", "Artist");
    let catalog = model::build(
        vec![ScannedFile {
            path: "/music/song.flac".into(),
            size: 123,
            mtime: 1,
            tags,
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/music".into()],
        1,
        &[],
    );
    store::save(&catalog, &store::catalog_path(&dir.0)).unwrap();
    let output = dir.0.join("reports/song.json");
    let result = Command::new(env!("CARGO_BIN_EXE_aede"))
        .args(["track", "Only song", "--json"])
        .arg(format!("--data={}", dir.0.display()))
        .arg(format!("--output={}", output.display()))
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result);
    assert!(
        output.is_file(),
        "JSON was printed instead of saved: {:?}",
        result
    );
    let saved = json::parse(&std::fs::read_to_string(&output).unwrap()).unwrap();
    let tracks = saved.as_arr().unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].field_str("title").as_deref(), Some("Only song"));
    assert_eq!(
        tracks[0].field_str("path").as_deref(),
        Some("/music/song.flac")
    );
    assert_eq!(tracks[0].field_u64("size"), Some(123));
    assert!(tracks[0].get("credits").unwrap().as_arr().is_some());
}

#[test]
fn unsafe_outputs_are_refused_before_a_manual_reattachment_is_saved() {
    let dir = std::env::temp_dir().join(format!("aede-export-preflight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let catalog = model::build(
        vec![ScannedFile {
            path: "/new/a.flac".into(),
            size: 1,
            mtime: 1,
            tags: tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: None,
            fingerprint: None,
        }],
        vec!["/new".into()],
        1,
        &[],
    );
    store::save(&catalog, &store::catalog_path(&dir)).unwrap();
    let from = EntityRef::new(EntityKind::Track, "/old/a.flac");
    let mut data = UserData::default();
    data.entry(LOCAL_USER, &from, 1).note = Some("keep the original attachment".into());
    user::save(&data, &user::user_path(&dir)).unwrap();
    let before = std::fs::read(user::user_path(&dir)).unwrap();
    let audio = dir.join("original.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../aede-core/tests/fixtures/track.flac"),
        &audio,
    )
    .unwrap();
    let audio_before = std::fs::read(&audio).unwrap();
    let outputs = vec![user::user_path(&dir), audio.clone(), dir.clone()];
    #[cfg(unix)]
    let outputs = {
        let link = dir.join("link.json");
        std::os::unix::fs::symlink(&audio, &link).unwrap();
        outputs
            .into_iter()
            .chain(std::iter::once(link))
            .collect::<Vec<_>>()
    };
    for output in outputs {
        let result = Command::new(env!("CARGO_BIN_EXE_aede"))
            .args([
                "notes",
                "--relink=track:/old/a.flac",
                "--to=track:/new/a.flac",
                "--json",
            ])
            .arg(format!("--data={}", dir.display()))
            .arg(format!("--output={}", output.display()))
            .env("NO_COLOR", "1")
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "accepted unsafe output {}",
            output.display()
        );
        assert_eq!(
            std::fs::read(user::user_path(&dir)).unwrap(),
            before,
            "the relink was saved before refusing {}: {}",
            output.display(),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(&audio).unwrap(), audio_before);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
