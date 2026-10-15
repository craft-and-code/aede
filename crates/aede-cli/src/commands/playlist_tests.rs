use super::*;
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
