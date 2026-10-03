//! Filesystem snapshots for public M1 command tests, with or without fetching.

use std::path::{Path, PathBuf};

pub fn snapshot(folder: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, folder: &Path, entries: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, entries);
            } else {
                entries.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut entries = Vec::new();
    visit(folder, folder, &mut entries);
    entries.sort();
    entries
}
