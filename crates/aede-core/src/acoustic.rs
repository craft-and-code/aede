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

/// A reusable upstream result together with its complete attributed source entry.
#[derive(Debug, Clone)]
pub struct SavedFileAnalysis {
    /// Typed fields used for cache validity and human presentation.
    pub analysis: FileAnalysis,
    /// Original measurement date, independent of later report rewrites.
    pub result_at_ns: u64,
    // Shared because one artist report may be reused by several album groups.
    source: std::sync::Arc<serde_json::Value>,
}

/// Read reusable results without discarding fields unknown to this upstream build.
///
/// The upstream reader validates its report contract, and the catalog reader
/// validates the interpreted measurements and exact per-entry dates. Raw source
/// entries remain separate from the typed projection used to select cache hits.
pub fn saved_report(
    text: &str,
    report_date: u64,
) -> Result<Vec<SavedFileAnalysis>, crate::analysis::ImportError> {
    use crate::analysis::ImportError;
    let report = fc::report::parse_json(text).map_err(ImportError::Json)?;
    let dates: Vec<_> = crate::analysis::parse_report_at(text, report_date)?
        .files
        .into_iter()
        .map(|file| file.result_at_ns)
        .collect();
    let mut value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| ImportError::Json(error.to_string()))?;
    let files = value
        .get_mut("report")
        .and_then(|report| report.get_mut("files"))
        .and_then(serde_json::Value::as_array_mut)
        .ok_or(ImportError::Malformed("report.files must be an array"))?;
    if report.files.len() != files.len() || dates.len() != files.len() {
        return Err(ImportError::Malformed(
            "report entries must have valid paths",
        ));
    }
    Ok(report
        .files
        .into_iter()
        .zip(dates)
        .zip(std::mem::take(files))
        .map(|((analysis, result_at_ns), source)| SavedFileAnalysis {
            analysis,
            result_at_ns,
            source: std::sync::Arc::new(source),
        })
        .collect())
}

/// Serialize an upstream report while preserving the date of every measurement.
///
/// Dates are decimal strings in an additive file-entry field ignored by upstream
/// readers. Reused results retain their original date, even in a newly written
/// report containing freshly decoded tracks. Reused entries retain all original
/// fields, including nested unknown measurements. Fresh entries keep upstream
/// float stabilization; integer precision is retained in both cases.
pub fn report_json_with_dates(
    report: &FolderReport,
    dates: &BTreeMap<String, u64>,
    reused: &BTreeMap<String, &SavedFileAnalysis>,
) -> Result<String, crate::analysis::ImportError> {
    use crate::analysis::{ImportError, RESULT_DATE_FIELD};
    let text =
        fc::report::build_json(report).map_err(|error| ImportError::Json(error.to_string()))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| ImportError::Json(error.to_string()))?;
    drop(text);
    let files = value
        .get_mut("report")
        .and_then(|report| report.get_mut("files"))
        .and_then(serde_json::Value::as_array_mut)
        .ok_or(ImportError::Malformed("report.files must be an array"))?;
    for file in files {
        let path = file
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or(ImportError::Malformed("report entries must have paths"))?;
        let date = dates.get(path).ok_or(ImportError::Malformed(
            "a measurement date is required for each report entry",
        ))?;
        if let Some(saved) = reused.get(path) {
            if saved.analysis.path != path
                || saved.source.get("path").and_then(serde_json::Value::as_str) != Some(path)
            {
                return Err(ImportError::Malformed(
                    "reused report entry has a different path",
                ));
            }
            *file = saved.source.as_ref().clone();
        }
        let fields = file
            .as_object_mut()
            .ok_or(ImportError::Malformed("report entries must be objects"))?;
        fields.insert(
            RESULT_DATE_FIELD.into(),
            serde_json::Value::String(date.to_string()),
        );
    }
    serde_json::to_string_pretty(&value).map_err(|error| ImportError::Json(error.to_string()))
}

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

/// Analyze an album while reporting each starting file to the caller.
///
/// The callback runs on an analysis worker before decoding its file, so
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
    let slots: Vec<OnceLock<FileAnalysis>> = (0..paths.len()).map(|_| OnceLock::new()).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads.max(1).min(paths.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= paths.len() {
                        break;
                    }
                    on_progress(index + 1, paths.len(), &paths[index]);
                    let _ = slots[index].set(fc::analyze_file(&paths[index], options));
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
