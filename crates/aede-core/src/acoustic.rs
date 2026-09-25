//! Acoustic analysis of catalogued albums with FlacCompagnon's Rust library.
//!
//! Album membership comes from Aède's catalog, including multi-disc releases.
//! Measurements retain FlacCompagnon's provenance in Aède's analysis table.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use flaccompagnon_core::{self as fc, FileAnalysis, FolderReport, ScanOptions};

use crate::model::{Catalog, Id};
use crate::text;

/// Group catalogued files by their physical album folder.
///
/// Multiple releases in one directory produce one report containing both.
/// A multi-disc release uses its shared parent folder.
pub fn album_files(catalog: &Catalog, scope: &[String]) -> BTreeMap<PathBuf, Vec<PathBuf>> {
    let mut by_file: BTreeMap<Id, PathBuf> = BTreeMap::new();
    for release in &catalog.releases {
        for track_id in &release.track_ids {
            if let Some(track) = catalog.tracks.get(*track_id as usize) {
                by_file.insert(track.file_id, PathBuf::from(&release.folder));
            }
        }
    }

    let mut folders: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    for file in &catalog.files {
        if !scope.is_empty() && !scope.iter().any(|root| text::is_under(&file.path, root)) {
            continue;
        }
        let path = PathBuf::from(&file.path);
        let folder = by_file
            .get(&file.id)
            .cloned()
            .or_else(|| path.parent().map(Path::to_path_buf));
        if let Some(folder) = folder {
            folders.entry(folder).or_default().insert(path);
        }
    }
    folders
        .into_iter()
        .map(|(folder, files)| (folder, files.into_iter().collect()))
        .collect()
}

/// Analyze an album's selected files concurrently, preserving catalog order.
pub fn analyze_album(
    folder: &Path,
    paths: &[PathBuf],
    options: &ScanOptions,
    threads: usize,
) -> FolderReport {
    analyze_album_with_progress(folder, paths, options, threads, |_, _, _| {})
}

/// Analyze an album while reporting each completed file to the caller.
///
/// The callback runs on an analysis worker after its file has been stored, so
/// a terminal client can show useful progress without changing ordering or
/// requiring the core to write to a terminal itself.
pub fn analyze_album_with_progress<F>(
    folder: &Path,
    paths: &[PathBuf],
    options: &ScanOptions,
    threads: usize,
    on_progress: F,
) -> FolderReport
where
    F: Fn(usize, usize, &Path) + Sync,
{
    let next = AtomicUsize::new(0);
    let completed = AtomicUsize::new(0);
    let slots: Vec<OnceLock<FileAnalysis>> = (0..paths.len()).map(|_| OnceLock::new()).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1).min(paths.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= paths.len() {
                        break;
                    }
                    let _ = slots[index].set(fc::analyze_file(&paths[index], options));
                    let done = completed.fetch_add(1, Ordering::Relaxed) + 1;
                    on_progress(done, paths.len(), &paths[index]);
                }
            });
        }
    });
    let files: Vec<FileAnalysis> = slots.into_iter().filter_map(OnceLock::into_inner).collect();
    let has_flac = files.iter().any(|file| file.flac_md5.is_some());
    FolderReport {
        root: folder.to_string_lossy().to_string(),
        files,
        has_flac,
    }
}

#[cfg(test)]
#[path = "acoustic_tests.rs"]
mod tests;
