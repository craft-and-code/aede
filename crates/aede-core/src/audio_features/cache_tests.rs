use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "aede-audio-cache-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn record() -> Record {
    let mut extract = Extractor::new(1).unwrap();
    extract.push(&vec![0.; 4096], || Ok(())).unwrap();
    Record {
        input: InputIdentity {
            path: std::env::temp_dir()
                .join("fictional-source.flac")
                .to_string_lossy()
                .into_owned(),
            size: 100,
            modified: 10,
            nanos: 15,
        },
        features: extract.finish().unwrap(),
    }
}
#[test]
fn completed_records_survive_restart_and_changed_sources_are_missing_not_reused() {
    let fixture = Fixture::new();
    let stored = record();
    save(&fixture.0, &stored).unwrap();
    assert_eq!(
        load(&fixture.0, &stored.input).unwrap().unwrap().features,
        stored.features
    );
    let mut changed = stored.input.clone();
    changed.nanos += 1;
    assert!(load(&fixture.0, &changed).unwrap().is_none());
    assert!(load(&fixture.0, &stored.input).unwrap().is_some());
}
#[test]
fn corrupt_or_wrong_profile_cache_is_refused_without_resetting_existing_bytes() {
    let fixture = Fixture::new();
    let stored = record();
    save(&fixture.0, &stored).unwrap();
    let path = location(&fixture.0, &stored.input);
    fs::write(&path, b"{broken").unwrap();
    assert!(load(&fixture.0, &stored.input).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"{broken");
    save(&fixture.0, &stored).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["features"]["profile"] = 999.into();
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(load(&fixture.0, &stored.input).is_err());
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[cfg(unix)]
#[test]
fn cache_symlinks_never_expose_or_overwrite_another_file() {
    let fixture = Fixture::new();
    let stored = record();
    save(&fixture.0, &stored).unwrap();
    let path = location(&fixture.0, &stored.input);
    let target = fixture.0.join("retained.json");
    fs::write(&target, b"retained").unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(load(&fixture.0, &stored.input).is_err());
    assert!(save(&fixture.0, &stored).is_err());
    assert_eq!(fs::read(target).unwrap(), b"retained");
}
