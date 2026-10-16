//! Tests for [`super`], split out of `check.rs`.
//!
//! About the wording of the interruption hint, which is where the previous
//! message went wrong: it claimed a continuous save without naming the batch
//! size, which is misleading — an interrupt mid-batch loses that whole batch,
//! not "at most the current file". The end-to-end behaviour (a batch actually
//! surviving a restart) is proved against the real binary elsewhere; this
//! guards the sentence a reader decides whether to trust.

use super::*;

struct Directory(std::path::PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn invalid_thread_counts_refuse_empty_and_cached_checks_without_changing_stores() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = Directory(
        std::env::temp_dir().join(format!("aede-check-threads-{}-{nonce}", std::process::id())),
    );
    std::fs::create_dir_all(&directory.0).unwrap();
    let path = directory.0.join("track.flac");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../aede-core/tests/fixtures/track.flac");
    std::fs::copy(source, &path).unwrap();
    let audio = std::fs::read(&path).unwrap();
    let cached = aede_core::model::build(
        vec![aede_core::model::ScannedFile {
            path: path.to_string_lossy().into_owned(),
            size: audio.len() as u64,
            mtime: 1,
            tags: aede_core::tags::RawTags::default(),
            folder_cover: None,
            sidecar: None,
            integrity: Some(IntegrityRecord {
                verdict: Verdict::Intact,
                method: "flac-frame-crc".into(),
                checked_at: 1,
            }),
            fingerprint: None,
        }],
        Vec::new(),
        1,
        &[],
    );
    let catalog_path = store::catalog_path(&directory.0);
    let arguments = |option: &str| {
        Args::parse([
            "check".to_owned(),
            format!("--data={}", directory.0.display()),
            option.to_owned(),
        ])
    };
    let snapshots = || {
        ["catalog.json", "conclusions.json"].map(|name| std::fs::read(directory.0.join(name)).ok())
    };
    for catalog in [&Catalog::default(), &cached] {
        store::save(catalog, &catalog_path).unwrap();
        let before = snapshots();
        for option in [
            "--threads=invalid",
            "--threads=-1",
            "--threads=1.5",
            "--threads=18446744073709551616",
        ] {
            assert!(check(&arguments(option)).is_err(), "accepted {option}");
            assert_eq!(snapshots(), before, "changed stores for {option}");
        }
        for option in ["--threads=0", "--threads=2"] {
            check(&arguments(option)).unwrap();
            assert_eq!(snapshots(), before, "changed stores for {option}");
        }
    }
    let mut pending = cached;
    pending.files[0].integrity = None;
    store::save(&pending, &catalog_path).unwrap();
    check(&arguments("--threads=2")).unwrap();
    assert_eq!(
        store::load(&catalog_path).unwrap().unwrap().files[0]
            .integrity
            .as_ref()
            .unwrap()
            .verdict,
        Verdict::Intact
    );
    assert_eq!(std::fs::read(&path).unwrap(), audio);
}

#[test]
fn interruption_hint_names_the_batch_size() {
    let hint = interruption_hint();
    assert!(
        hint.contains(&SAVE_EVERY.to_string()),
        "the hint must name the actual batch size, not just claim safety: {hint}"
    );
}

#[test]
fn interruption_hint_does_not_overclaim_continuous_saving() {
    // The old wording ("saved as the run goes") read as per-file safety, which
    // is not what the code does: a batch is committed as a unit.
    let hint = interruption_hint();
    assert!(
        !hint.contains("as the run goes"),
        "must not suggest saving happens continuously rather than per batch: {hint}"
    );
}
