//! Run FlacCompagnon's acoustic analysis on catalogued album folders.

use std::path::{Path, PathBuf};

use aede_core::acoustic;
use aede_core::analysis;
use aede_core::clock::now_seconds;
use aede_core::{ffmpeg, scan, store};
use flaccompagnon_core::{self as fc, ScanOptions};

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui;

/// Save one report per album with the same album-based stem as the desktop
/// application, without adding a tool name that every report already carries.
fn report_path(folder: &Path) -> PathBuf {
    let stem = folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "analysis".to_string());
    folder.join(format!("{stem}.json"))
}

fn report_destination(folder: &Path, layout: &str) -> PathBuf {
    let album_report = report_path(folder);
    if layout == "artist" {
        folder
            .parent()
            .unwrap_or(folder)
            .join(album_report.file_name().unwrap_or_default())
    } else {
        album_report
    }
}

#[derive(Debug)]
enum ReportDestinationError {
    Reserved(PathBuf),
    Io(std::io::Error),
}

impl From<std::io::Error> for ReportDestinationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl std::fmt::Display for ReportDestinationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reserved(path) => write!(
                f,
                "{}: analysis report would replace an Aède data file; choose another JSON layout or data directory",
                path.display()
            ),
            Self::Io(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ReportDestinationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reserved(_) => None,
            Self::Io(error) => Some(error),
        }
    }
}

fn validate_report_destination(
    destination: &Path,
    data_root: &Path,
) -> Result<(), ReportDestinationError> {
    let parent = destination.parent().unwrap_or(Path::new("."));
    let resolved_parent = parent.canonicalize()?;
    if resolved_parent.as_path() != data_root {
        return Ok(());
    }
    let reserved = [
        store::catalog_path(data_root),
        aede_core::conclusions::conclusions_path(data_root),
        aede_core::sources::sources_path(data_root),
        aede_core::user::user_path(data_root),
        aede_core::accounts::accounts_path(data_root),
        aede_core::parental::path(data_root),
    ];
    // Reserve these names on case-sensitive hosts too, so the same plan is safe
    // on case-insensitive filesystems. Resolving the parent covers path aliases.
    if let Some(name) = destination.file_name()
        && reserved.iter().any(|path| {
            path.file_name().is_some_and(|reserved| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case(&reserved.to_string_lossy())
            })
        })
    {
        return Err(ReportDestinationError::Reserved(destination.to_path_buf()));
    }
    Ok(())
}

fn show_result(file: &fc::FileAnalysis) {
    println!("{}", result_line(file));
}

fn result_line(file: &fc::FileAnalysis) -> String {
    let result = file.error.as_deref().unwrap_or(&file.detections.detail);
    format!(
        "  {} — {}: {}",
        ui::literal(&file.file_name),
        ui::literal(&file.detections.summary),
        ui::literal(result)
    )
}

fn show_progress(completed: usize, total: usize, path: &Path) {
    eprintln!("{}", progress_line(completed, total, path));
}

fn progress_line(completed: usize, total: usize, path: &Path) -> String {
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    format!("  ▸ [{completed}/{total}] Starting {}", ui::literal(&name))
}

pub fn analyze(args: &Args) -> Res {
    analyze_with_progress(args, show_progress)
}

fn analyze_with_progress(args: &Args, progress: impl Fn(usize, usize, &Path) + Sync + Send) -> Res {
    if args.has("json-layout") && args.value("json-layout").is_none() {
        return Err("--json-layout needs album or artist".into());
    }
    for flag in ["force", "show-results"] {
        if args.value(flag).is_some() {
            return Err(format!("--{flag} does not accept a value").into());
        }
    }
    let layout = args.value("json-layout").unwrap_or("album");
    if !matches!(layout, "album" | "artist") {
        return Err("--json-layout must be album or artist".into());
    }
    eprintln!("Preparing acoustic analysis…");
    let catalog = ui::with_loading("Loading catalog…", || load(args))?;
    let scope = super::scope_of(args, &catalog)?;
    let albums = acoustic::album_files(&catalog, &scope);
    if albums.is_empty() {
        return Err("no catalogued audio files in the selected folders".into());
    }
    let data_root = if args.has("json") || args.has("json-layout") {
        Some(data_dir(args).canonicalize()?)
    } else {
        None
    };
    if let Some(data_root) = &data_root {
        for folder in albums.keys() {
            match validate_report_destination(&report_destination(folder, layout), data_root) {
                Ok(()) | Err(ReportDestinationError::Io(_)) => {
                    // A missing volume is an album failure, not a reason to
                    // abandon other folders. Revalidate at publication below.
                }
                Err(error @ ReportDestinationError::Reserved(_)) => return Err(error.into()),
            }
        }
    }

    let threads = scan::resolve_threads(args.number_or("threads", 0)?);
    let options = ScanOptions {
        ffmpeg: ffmpeg::find(),
        ..ScanOptions::default()
    };
    let mut completed = 0usize;
    let mut failed_files = 0usize;
    let mut changed_cached_files = 0usize;
    let mut failed_reports = Vec::new();
    let mut snapshots = std::collections::BTreeMap::new();
    println!("{}", ui::section("Acoustic analysis (FlacCompagnon)"));

    for (folder, paths) in albums {
        let destination = report_destination(&folder, layout);
        let mut cached = Vec::new();
        ui::with_loading("Reading saved results…", || -> Res {
            if !args.has("force") {
                let mut candidates = vec![report_path(&folder), destination.clone()];
                for dir in [Some(folder.as_path()), folder.parent()]
                    .into_iter()
                    .flatten()
                {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        candidates.extend(
                            entries
                                .filter_map(Result::ok)
                                .map(|e| e.path())
                                .filter(|p| p.extension().is_some_and(|e| e == "json")),
                        );
                    }
                }
                candidates.sort();
                candidates.dedup();
                candidates.sort_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
                for path in candidates.into_iter().filter(|p| p.is_file()) {
                    // The same artist report may cover many albums: decode it once per run.
                    let snapshot = snapshots.entry(path.clone()).or_insert_with(|| {
                        let text =
                            std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
                        let metadata =
                            std::fs::metadata(&path).map_err(|error| error.to_string())?;
                        acoustic::saved_report(
                            &text,
                            aede_core::clock::mtime_nanoseconds(&metadata),
                        )
                        .map_err(|error| error.to_string())
                    });
                    match snapshot {
                        Ok(report) => {
                            for file in report {
                                if paths
                                    .iter()
                                    .any(|path| path.to_string_lossy() == file.analysis.path)
                                {
                                    cached.push(file.clone());
                                }
                            }
                        }
                        Err(error)
                            if path == destination
                                && (args.has("json") || args.has("json-layout")) =>
                        {
                            return Err(format!(
                                "{}: {error}; use --force to replace it",
                                path.display()
                            )
                            .into());
                        }
                        Err(_) => {}
                    }
                }
            }
            Ok(())
        })?;
        // A recent report can contain old reused results. Select by measurement
        // date, and keep the selected row's date beside its payload, not by path.
        cached.sort_by_key(|file| file.result_at_ns);
        cached.reverse();
        let pending: Vec<_> = paths
            .iter()
            .filter(|path| !cached.iter().any(|file| reusable(&file.analysis, path)))
            .cloned()
            .collect();
        for path in paths.iter().filter(|path| !pending.contains(path)) {
            eprintln!("  ▸ Reusing {}", ui::literal(&path.to_string_lossy()));
        }
        let changed = !pending.is_empty();
        if changed {
            eprintln!(
                "  ▸ [0/{}] Analyzing {}",
                pending.len(),
                ui::literal(&folder.to_string_lossy())
            );
        }
        let mut report =
            acoustic::analyze_album_with_progress(&folder, &pending, &options, threads, &progress);
        let result_at_ns = aede_core::clock::now_nanoseconds();
        let mut result_dates: std::collections::BTreeMap<_, _> = report
            .files
            .iter()
            .map(|file| (file.path.clone(), result_at_ns))
            .collect();
        let mut reused = std::collections::BTreeMap::new();
        let mut changed_cache = 0usize;
        // A decoded file already has a new result. A cache that becomes valid
        // again during decoding must not append a duplicate or replace its date.
        for path in paths.iter().filter(|path| !pending.contains(path)) {
            if let Some(file) = cached.iter().find(|file| reusable(&file.analysis, path)) {
                result_dates.insert(file.analysis.path.clone(), file.result_at_ns);
                reused.insert(file.analysis.path.clone(), file);
                report.files.push(file.analysis.clone());
            } else {
                changed_cache += 1;
                eprintln!(
                    "  Cached audio changed or disappeared: {}; scan and retry",
                    ui::literal(&path.to_string_lossy())
                );
            }
        }
        changed_cached_files += changed_cache;
        report.files.sort_by(|a, b| a.path.cmp(&b.path));
        report.has_flac = report.files.iter().any(|file| file.flac_md5.is_some());
        failed_files += report
            .files
            .iter()
            .filter(|file| file.error.is_some())
            .count();
        if args.has("show-results") {
            for file in &report.files {
                show_result(file);
            }
        }
        let json = acoustic::report_json_with_dates(&report, &result_dates, &reused)?;
        let imported = analysis::parse_report(&json)?;
        // Decoding can take hours; only the atomic merge owns the shared writer.
        // Reload here so a concurrent scan or metadata publication survives.
        let attachment = {
            let _guard = aede_core::store_lock::StoreLock::acquire(&data_dir(args))?;
            let mut latest = load(args)?;
            let attachment = analysis::merge_into(&mut latest, imported.files, now_seconds());
            store::save(&latest, &store::catalog_path(&data_dir(args)))?;
            attachment
        };

        // Retain an existing report rather than publishing a silently
        // incomplete replacement after cache validation changed mid-run.
        if let Some(data_root) = &data_root
            && changed_cache == 0
            && (changed || !destination.exists())
        {
            let written = validate_report_destination(&destination, data_root)
                .map_err(|error| error.to_string())
                .and_then(|()| {
                    fc::report::write_atomic_bytes(&destination, json.as_bytes())
                        .map_err(|error| error.to_string())
                });
            if let Err(error) = written {
                failed_reports.push((destination, error));
            }
        }

        completed += 1;
        println!(
            "  {} — {} track(s)",
            ui::literal(&folder.to_string_lossy()),
            paths.len()
        );
        if attachment.stale > 0 {
            eprintln!(
                "  {} result(s) changed while being analyzed",
                attachment.stale
            );
        }
        if attachment.older > 0 {
            eprintln!("  {} newer stored result(s) retained", attachment.older);
        }
    }

    println!("  {completed} album folder(s) processed");
    if failed_files > 0 {
        eprintln!("  {failed_files} file(s) could not be decoded; their errors are in the reports");
    }
    if changed_cached_files > 0 {
        eprintln!(
            "  {changed_cached_files} cached file(s) changed or disappeared; incomplete album reports were not written"
        );
    }
    for (path, error) in &failed_reports {
        eprintln!(
            "  {}: {}",
            ui::literal(&path.to_string_lossy()),
            ui::literal(error)
        );
    }
    if failed_files > 0 || changed_cached_files > 0 || !failed_reports.is_empty() {
        return Err("some analyses or report writes failed".into());
    }
    Ok(())
}

fn reusable(file: &fc::FileAnalysis, path: &Path) -> bool {
    // Native analysis requests every measurement, even when a valid upstream
    // report was deliberately restricted to a smaller selection.
    if file.error.is_some()
        || !file.decoder_is_current()
        || !file.covers_selection(fc::AnalysisSelection::all())
        || Path::new(&file.path) != path
    {
        return false;
    }
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|t| i64::try_from(t.as_secs()).ok());
    file.size_bytes == metadata.len()
        && file.modified_unix.is_some()
        && file.modified_unix == modified
}

#[cfg(test)]
#[path = "analyze_tests.rs"]
mod tests;
