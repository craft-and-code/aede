use super::test_support::Sandbox;
use super::*;
use std::collections::BTreeSet;

pub(super) fn canonical_sandbox() -> Sandbox {
    let mut sandbox = Sandbox::new();
    sandbox.0 = sandbox.0.canonicalize().unwrap();
    sandbox
}

pub(super) fn assert_paths(catalog: &Catalog, expected: &[PathBuf]) {
    let paths: BTreeSet<&Path> = catalog
        .files
        .iter()
        .map(|file| Path::new(&file.path))
        .collect();
    assert_eq!(paths, expected.iter().map(PathBuf::as_path).collect());
    assert_eq!(catalog.tracks.len(), expected.len());
}

pub(super) fn assert_cached_file(previous: &Catalog, catalog: &Catalog, path: &Path) {
    let before = previous
        .files
        .iter()
        .find(|file| Path::new(&file.path) == path)
        .unwrap();
    let after = catalog
        .files
        .iter()
        .find(|file| Path::new(&file.path) == path)
        .unwrap();
    assert_eq!(after.path, before.path, "stored path spelling must survive");
    assert_eq!(after.size, before.size);
    assert_eq!(after.mtime, before.mtime);
    assert_eq!(after.tags, before.tags);
    assert_eq!(
        catalog.file_mtime_subseconds.get(&after.path),
        previous.file_mtime_subseconds.get(&before.path)
    );
    assert_eq!(
        catalog.file_added_at.get(&after.path),
        previous.file_added_at.get(&before.path)
    );
}

pub(super) fn assert_unavailable(report: &ScanReport, path: &Path) {
    assert_eq!(
        report.failures.len(),
        1,
        "only the declared subtree is unavailable: {:?}",
        report.failures
    );
    assert_eq!(Path::new(&report.failures[0].0), path);
    assert!(report.failures[0].1.contains("not mounted"));
}
