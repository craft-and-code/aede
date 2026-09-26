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
