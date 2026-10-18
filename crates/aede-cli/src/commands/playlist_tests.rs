use super::*;

#[test]
fn an_unavailable_album_folder_is_not_recreated_by_playlist_generation() {
    let sandbox =
        std::env::temp_dir().join(format!("aede_playlist_unavailable_{}", std::process::id()));
    let missing = sandbox.join("offline");
    let online = sandbox.join("online");
    let data = sandbox.join("data");
    let bytes = include_bytes!("../../../aede-core/tests/fixtures/track.flac");
    for album in [&missing, &online] {
        std::fs::create_dir_all(album).unwrap();
        std::fs::write(album.join("01.flac"), bytes).unwrap();
    }
    let (catalog, _) = aede_core::scan::scan(
        &[missing.clone(), online.clone()],
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    let catalog_path = aede_core::store::catalog_path(&data);
    aede_core::store::save(&catalog, &catalog_path).unwrap();
    let catalog_bytes = std::fs::read(&catalog_path).unwrap();
    let parked = sandbox.join("unmounted");
    std::fs::rename(&missing, &parked).unwrap();

    let args = Args::parse(vec![
        "playlist".into(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
    ]);
    let result = playlist(&args);
    assert!(result.is_err(), "the unavailable album is still a failure");
    assert!(
        !missing.exists(),
        "do not recreate an unavailable library folder"
    );
    let content = std::fs::read_to_string(online.join("online.m3u")).unwrap();
    assert_eq!(lines(&content), 1, "available albums are still published");
    assert!(content.lines().any(|line| line == "01.flac"));
    assert_eq!(std::fs::read(online.join("01.flac")).unwrap(), bytes);
    assert_eq!(std::fs::read(parked.join("01.flac")).unwrap(), bytes);
    assert_eq!(std::fs::read(&catalog_path).unwrap(), catalog_bytes);
    std::fs::remove_dir_all(sandbox).unwrap();
}

#[test]
fn playlist_write_failures_are_returned_to_the_caller() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sandbox = std::env::temp_dir().join(format!(
        "aede_playlist_error_{}_{nonce}",
        std::process::id()
    ));
    let music = sandbox.join("Album");
    let data = sandbox.join("data");
    std::fs::create_dir_all(&music).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac"),
        music.join("01.flac"),
    )
    .unwrap();
    let (catalog, _) = aede_core::scan::scan(
        std::slice::from_ref(&music),
        None,
        &Default::default(),
        |_| {},
    )
    .unwrap();
    aede_core::store::save(&catalog, &aede_core::store::catalog_path(&data)).unwrap();
    std::fs::create_dir(music.join("Album.m3u")).unwrap();
    let args = Args::parse(vec![
        "playlist".into(),
        "--data".into(),
        data.to_string_lossy().into_owned(),
    ]);
    let result = playlist(&args);
    std::fs::remove_dir_all(&sandbox).unwrap();
    assert!(
        result.is_err(),
        "a failed write must give a failing command status"
    );
}
