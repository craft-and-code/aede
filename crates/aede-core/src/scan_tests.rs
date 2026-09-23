use super::*;

#[test]
fn cover_art_rank() {
    assert_eq!(cover_rank("cover.jpg"), Some(0));
    assert_eq!(cover_rank("folder.png"), Some(1));
    assert_eq!(cover_rank("scan-back.jpg"), Some(COVER_NAMES.len()));
    assert_eq!(cover_rank("notes.txt"), None);
    assert_eq!(cover_rank("no_extension"), None);
}

#[test]
fn scan_of_a_real_folder() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let (catalog, report) = scan(&[root], None, &ScanOptions::default(), |_| {}).expect("scan");

    assert!(report.found >= 8, "files found: {}", report.found);
    assert_eq!(report.read, report.found);
    assert_eq!(report.reused, 0);
    assert!(
        report.failures.is_empty(),
        "failures: {:?}",
        report.failures
    );
    assert_eq!(catalog.files.len(), report.found);
    assert!(catalog.find_artist("Miles Davis").is_some());
}

#[test]
fn second_scan_reuses_everything() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let (first, _) = scan(
        std::slice::from_ref(&root),
        None,
        &ScanOptions::default(),
        |_| {},
    )
    .unwrap();
    let (second, report) = scan(&[root], Some(&first), &ScanOptions::default(), |_| {}).unwrap();

    assert_eq!(report.read, 0, "no file should have been read again");
    assert_eq!(report.reused, report.found);
    // And the rebuilt catalog must be identical.
    assert_eq!(first.files.len(), second.files.len());
    assert_eq!(first.artists.len(), second.artists.len());
    assert_eq!(first.tracks.len(), second.tracks.len());
}
