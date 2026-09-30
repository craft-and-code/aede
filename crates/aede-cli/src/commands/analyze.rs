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

fn show_result(file: &fc::FileAnalysis) {
    let result = file.error.as_deref().unwrap_or(&file.detections.detail);
    println!(
        "  {} — {}: {result}",
        file.file_name, file.detections.summary
    );
}

fn show_progress(completed: usize, total: usize, path: &Path) {
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    eprintln!("  ▸ [{completed}/{total}] Starting {name}");
}

pub fn analyze(args: &Args) -> Res {
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
    let mut catalog = ui::with_loading("Loading catalog…", || load(args))?;
    let scope = super::scope_of(args, &catalog)?;
    let albums = acoustic::album_files(&catalog, &scope);
    if albums.is_empty() {
        return Err("no catalogued audio files in the selected folders".into());
    }

    let threads = scan::resolve_threads(args.number_or("threads", 0)?);
    let options = ScanOptions {
        ffmpeg: ffmpeg::find(),
        ..ScanOptions::default()
    };
    let catalog_file = store::catalog_path(&data_dir(args));
    let mut completed = 0usize;
    let mut failed_files = 0usize;
    let mut failed_reports = Vec::new();
    let mut snapshots = std::collections::BTreeMap::new();
    println!("{}", ui::section("Acoustic analysis (FlacCompagnon)"));

    for (folder, paths) in albums {
        let destination = if layout == "artist" {
            folder
                .parent()
                .unwrap_or(&folder)
                .join(report_path(&folder).file_name().unwrap_or_default())
        } else {
            report_path(&folder)
        };
        let mut cached = Vec::new();
        let mut saved_dates = std::collections::BTreeMap::new();
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
                        let report = fc::report::parse_json(&text)?;
                        let metadata =
                            std::fs::metadata(&path).map_err(|error| error.to_string())?;
                        Ok::<_, String>((report, aede_core::clock::mtime_nanoseconds(&metadata)))
                    });
                    match snapshot {
                        Ok((report, date)) => {
                            for file in &report.files {
                                if paths.iter().any(|path| path.to_string_lossy() == file.path) {
                                    saved_dates.insert(file.path.clone(), *date);
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
        cached.reverse();
        let pending: Vec<_> = paths
            .iter()
            .filter(|path| !cached.iter().any(|file| reusable(file, path)))
            .cloned()
            .collect();
        for path in paths.iter().filter(|path| !pending.contains(path)) {
            eprintln!("  ▸ Reusing {}", path.display());
        }
        let changed = !pending.is_empty();
        if changed {
            eprintln!("  ▸ [0/{}] Analyzing {}", pending.len(), folder.display());
        }
        let mut report = acoustic::analyze_album_with_progress(
            &folder,
            &pending,
            &options,
            threads,
            show_progress,
        );
        for path in &paths {
            if let Some(file) = cached.iter().find(|file| reusable(file, path)) {
                report.files.push(file.clone());
            }
        }
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
        let json = fc::report::build_json(&report)?;
        let mut imported = analysis::parse_report(&json)?;
        let result_at_ns = aede_core::clock::now_nanoseconds();
        for file in &mut imported.files {
            file.result_at_ns = if pending
                .iter()
                .any(|path| path.to_string_lossy() == file.path)
            {
                result_at_ns
            } else {
                saved_dates.get(&file.path).copied().unwrap_or(0)
            };
        }
        let attachment = analysis::merge_into(&mut catalog, imported.files, now_seconds());
        store::save(&catalog, &catalog_file)?;

        if (args.has("json") || args.has("json-layout"))
            && (changed || !destination.exists())
            && let Err(error) = fc::report::write_json(&destination, &report)
        {
            failed_reports.push((destination, error.to_string()));
        }

        completed += 1;
        println!("  {} — {} track(s)", folder.display(), paths.len());
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
    for (path, error) in &failed_reports {
        eprintln!("  {}: {error}", path.display());
    }
    if failed_files > 0 || !failed_reports.is_empty() {
        return Err("some analyses or report writes failed".into());
    }
    Ok(())
}

fn reusable(file: &fc::FileAnalysis, path: &Path) -> bool {
    if file.error.is_some() || Path::new(&file.path) != path {
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
