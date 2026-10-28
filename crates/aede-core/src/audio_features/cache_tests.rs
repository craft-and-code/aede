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
fn legacy_decoder_records_are_not_reused_or_deleted() {
    let fixture = Fixture::new();
    let stored = record();
    let input = &stored.input;
    let token = format!(
        "{}\0{}\0{}\0{}",
        input.path, input.size, input.modified, input.nanos
    );
    let legacy = fixture
        .0
        .join(format!("acoustic-v{}", aede_moods::PROFILE))
        .join(format!("{:x}.json", Md5::digest(token.as_bytes())));
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({"path":input.path,"size":input.size,"modified":input.modified,"nanos":input.nanos,"features":stored.features})).unwrap();
    fs::write(&legacy, &bytes).unwrap();
    assert!(
        load(&fixture.0, input).unwrap().is_none(),
        "old PCM scale must not survive decoder correction"
    );
    assert_eq!(fs::read(&legacy).unwrap(), bytes);
    save(&fixture.0, &stored).unwrap();
    assert_eq!(load(&fixture.0, input).unwrap().unwrap(), stored);
}
#[test]
fn decoder_expiry_withholds_training_vectors_without_changing_private_labels() {
    let fixture = Fixture::new();
    let labels = fixture.0.join("user.json");
    let private = br#"{"owner":"alice","tags":["calm"]}"#;
    fs::write(&labels, private).unwrap();
    let mut first = record();
    first.features.frames = 8192;
    first.features.windows = 8;
    first.features.rms_dbfs = Some(-20.);
    let mut second = first.clone();
    second.input.path.push_str(".second");
    assert!(aede_moods::AcousticModel::new(&[&first.features, &second.features]).is_some());
    for stored in [&first, &second] {
        let input = &stored.input;
        let token = format!(
            "{}\0{}\0{}\0{}",
            input.path, input.size, input.modified, input.nanos
        );
        let legacy = fixture
            .0
            .join(format!("acoustic-v{}", aede_moods::PROFILE))
            .join(format!("{:x}.json", Md5::digest(token.as_bytes())));
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, serde_json::to_vec(&serde_json::json!({"path":input.path,"size":input.size,"modified":input.modified,"nanos":input.nanos,"features":stored.features})).unwrap()).unwrap();
    }
    let current: Vec<_> = [&first, &second]
        .into_iter()
        .filter_map(|record| load(&fixture.0, &record.input).unwrap())
        .collect();
    let seeds: Vec<_> = current.iter().map(|record| &record.features).collect();
    assert!(aede_moods::AcousticModel::new(&seeds).is_none());
    for stored in [&first, &second] {
        save(&fixture.0, stored).unwrap();
    }
    let refreshed: Vec<_> = [&first, &second]
        .into_iter()
        .map(|record| load(&fixture.0, &record.input).unwrap().unwrap())
        .collect();
    assert!(
        aede_moods::AcousticModel::new(
            &refreshed
                .iter()
                .map(|record| &record.features)
                .collect::<Vec<_>>()
        )
        .is_some()
    );
    assert_eq!(fs::read(labels).unwrap(), private);
    assert_eq!(aede_moods::PROFILE, 1);
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
