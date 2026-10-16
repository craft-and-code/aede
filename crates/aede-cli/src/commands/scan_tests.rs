use super::*;

#[test]
fn a_full_scan_keeps_saved_folder_exclusions() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let sandbox = std::env::temp_dir().join(format!(
        "aede_full_exclusions_{}_{nonce}",
        std::process::id()
    ));
    let music = sandbox.join("music");
    let excluded = music.join("excluded");
    let data = sandbox.join("data");
    std::fs::create_dir_all(&excluded).expect("excluded folder");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        excluded.join("track.flac"),
    )
    .expect("audio fixture");
    let music = music.canonicalize().expect("music path");
    let excluded = excluded.canonicalize().expect("excluded path");
    let catalog = Catalog {
        roots: vec![music.to_string_lossy().into_owned()],
        excluded: vec![excluded.to_string_lossy().into_owned()],
        ..Default::default()
    };
    let path = store::catalog_path(&data);
    store::save(&catalog, &path).expect("save watched folders");
    let args = Args::parse(vec![
        "scan".into(),
        "--full".into(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
    ]);
    scan(&args).expect("full scan");
    let refreshed = store::load(&path).expect("read catalog").expect("catalog");
    assert_eq!(refreshed.excluded, catalog.excluded);
    assert!(
        refreshed.files.is_empty(),
        "excluded audio must never enter the catalog"
    );
    std::fs::remove_dir_all(sandbox).expect("remove isolated test data");
}

#[test]
fn rescan_forwards_discovery_and_file_read_progress() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let sandbox = std::env::temp_dir().join(format!(
        "aede_scan_progress_{}_{}",
        std::process::id(),
        nonce
    ));
    let music = sandbox.join("music");
    let data = sandbox.join("data");
    std::fs::create_dir_all(&music).expect("music folder");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        music.join("track.flac"),
    )
    .expect("audio fixture");
    let first = Args::parse(vec![
        "scan".into(),
        music.to_string_lossy().into_owned(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
    ]);
    scan(&first).expect("initial scan");

    let repeat = Args::parse(vec![
        "scan".into(),
        "--full".into(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
    ]);
    let mut events = Vec::new();
    rescan_with_progress(&repeat, &mut |event| events.push(event)).expect("full rescan");
    assert!(matches!(events.first(), Some(Progress::Discovered(1))));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Progress::Read { done: 1, total: 1 }))
    );
    std::fs::remove_dir_all(sandbox).expect("remove isolated test data");
}

#[test]
fn a_full_scan_does_not_restore_a_waiting_copy_of_an_analysis_it_just_attached() {
    use aede_core::analysis::{self, FileAnalysis};
    use aede_core::json::Json;

    fn entry(path: &Path, detail: &str) -> Json {
        let metadata = std::fs::metadata(path).unwrap();
        let mut value = Json::obj();
        value.set("path", path.to_str().unwrap().into());
        value.set("size_bytes", metadata.len().into());
        value.set(
            "modified_unix",
            aede_core::clock::mtime_seconds(&metadata).into(),
        );
        value.set(
            "file_md5",
            flaccompagnon_core::file_digest(path).unwrap().md5.into(),
        );
        let mut detections = Json::obj();
        detections.set("summary", "Clean".into());
        detections.set("detail", detail.into());
        value.set("detections", detections);
        value.set("unknown_measurement", true.into());
        value
    }

    fn report(root: &Path, entry: Json) -> Json {
        let mut inner = Json::obj();
        inner.set("root", root.to_str().unwrap().into());
        inner.set("files", Json::Arr(vec![entry]));
        let mut outer = Json::obj();
        outer.set("format", analysis::FLACCOMPAGNON_FORMAT.into());
        outer.set("version", 1u32.into());
        outer.set("report", inner);
        outer
    }

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::temp_dir().join(format!(
        "aede_full_analysis_carryover_{}_{nonce}",
        std::process::id()
    ));
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures");
    let original_bytes = std::fs::read(fixtures.join("track.flac")).unwrap();
    let moved_bytes = std::fs::read(fixtures.join("hires.flac")).unwrap();
    for (index, (remove_catalog, fresh_report)) in [(false, false), (false, true), (true, false)]
        .into_iter()
        .enumerate()
    {
        let sandbox = base.join(index.to_string());
        let music = sandbox.join("music");
        let data = sandbox.join("data");
        std::fs::create_dir_all(&music).unwrap();
        let music = music.canonicalize().unwrap();
        let existing = music.join("existing.flac");
        std::fs::write(&existing, &original_bytes).unwrap();
        let (mut previous, _) = scan::scan(
            std::slice::from_ref(&music),
            None,
            &Default::default(),
            |_| {},
        )
        .unwrap();
        let checked = aede_core::audit::integrity::check(&existing).unwrap();
        let integrity = aede_core::model::IntegrityRecord {
            verdict: checked.verdict,
            method: checked.method.into(),
            checked_at: 10,
        };
        previous.files[0].integrity = Some(integrity.clone());

        let before_move = sandbox.join("before-move.flac");
        std::fs::write(&before_move, &moved_bytes).unwrap();
        let retained_entry = entry(&before_move, "retained result");
        let mut waiting: FileAnalysis =
            analysis::parse_report(&report(&sandbox, retained_entry.clone()).to_string_compact())
                .unwrap()
                .files
                .remove(0);
        waiting.result_at_ns = 100;
        assert_eq!(
            analysis::merge_into(&mut previous, vec![waiting], 20).waiting,
            1
        );
        let catalog_path = store::catalog_path(&data);
        store::save(&previous, &catalog_path).unwrap();
        if remove_catalog {
            std::fs::remove_file(&catalog_path).unwrap();
        }

        let moved = music.join("moved.flac");
        std::fs::rename(&before_move, &moved).unwrap();
        let expected_entry = if fresh_report {
            let fresh = entry(&moved, "fresh result");
            std::fs::write(
                music.join("fresh.json"),
                report(&music, fresh.clone()).to_string_compact(),
            )
            .unwrap();
            fresh
        } else {
            retained_entry
        };
        let args = Args::parse([
            "scan".to_string(),
            music.to_str().unwrap().to_string(),
            "--full".to_string(),
            "--threads".to_string(),
            "1".to_string(),
            "--data".to_string(),
            data.to_str().unwrap().to_string(),
        ]);
        scan(&args).unwrap();
        let refreshed = store::load(&catalog_path).unwrap().unwrap();
        assert_eq!(refreshed.analyses.len(), if fresh_report { 2 } else { 1 });
        assert_eq!(refreshed.pending_analyses(), usize::from(fresh_report));
        let moved_file = refreshed
            .files
            .iter()
            .find(|file| file.path == moved.to_str().unwrap())
            .unwrap();
        let attached = refreshed.analyses_of(moved_file).collect::<Vec<_>>();
        assert_eq!(attached.len(), 1);
        assert!(attached[0].still_applies(moved_file.size, moved_file.mtime));
        assert_eq!(attached[0].source_data, Some(expected_entry));
        let unchanged = refreshed
            .files
            .iter()
            .find(|file| file.path == existing.to_str().unwrap())
            .unwrap();
        assert_eq!(unchanged.integrity, Some(integrity));
        assert_eq!(std::fs::read(&existing).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&moved).unwrap(), moved_bytes);
    }
    std::fs::remove_dir_all(base).unwrap();
}
