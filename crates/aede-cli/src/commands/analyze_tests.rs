use super::*;

#[test]
fn report_path_uses_the_album_folder_name_without_a_tool_suffix() {
    assert_eq!(
        report_path(Path::new("/music/Artist/Album")),
        PathBuf::from("/music/Artist/Album/Album.json")
    );
}

#[test]
fn report_path_keeps_dots_in_an_album_name() {
    assert_eq!(
        report_path(Path::new("/music/Album.v1")),
        PathBuf::from("/music/Album.v1/Album.v1.json")
    );
}

#[test]
fn invalid_analysis_options_fail_before_catalog_loading() {
    for options in [
        vec!["analyze", "--json-layout", "wrong"],
        vec!["analyze", "--json-layout"],
        vec!["analyze", "--force=wrong"],
    ] {
        let args = Args::parse(options.into_iter().map(str::to_string));
        assert!(analyze(&args).is_err());
    }
}

#[test]
fn analysis_publication_preserves_a_writer_that_ran_while_audio_was_decoding() {
    let dir =
        std::env::temp_dir().join(format!("aede-analysis-publication-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let album = dir.join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../aede-core/tests/fixtures/track.flac");
    std::fs::copy(fixture, album.join("Track.flac")).unwrap();
    let (catalog, _) = scan::scan(&[album], None, &scan::ScanOptions::default(), |_| {}).unwrap();
    let path = store::catalog_path(&dir);
    store::save(&catalog, &path).unwrap();
    let args = Args::parse([
        "analyze".to_string(),
        "--data".to_string(),
        dir.to_str().unwrap().to_string(),
    ]);
    analyze_with_progress(&args, |_, _, _| {
        let _guard = aede_core::store_lock::StoreLock::try_acquire(&dir)
            .expect("decoder must not hold writer lock");
        let mut current = store::load(&path).unwrap().unwrap();
        current
            .excluded
            .push(dir.join("another-writer").to_str().unwrap().to_string());
        store::save(&current, &path).unwrap();
    })
    .unwrap();
    let saved = store::load(&path).unwrap().unwrap();
    assert_eq!(
        saved.excluded,
        [dir.join("another-writer").to_str().unwrap()]
    );
    assert_eq!(saved.analyses.len(), 1);
    assert_eq!(saved.analyses[0].md5_state.as_deref(), Some("Match"));
    std::fs::remove_dir_all(dir).unwrap();
}
