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
    const MARKERS: [&str; 4] = ["▸", "▹", "▪", "▫"];
    let marker = MARKERS[completed.saturating_sub(1) % MARKERS.len()];
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    eprintln!("  {marker} [{completed}/{total}] Analyzing {name}");
}

pub fn analyze(args: &Args) -> Res {
    let mut catalog = load(args)?;
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
    println!("{}", ui::section("Acoustic analysis (FlacCompagnon)"));

    for (folder, paths) in albums {
        eprintln!("  ▸ [0/{}] Analyzing {}", paths.len(), folder.display());
        let report = acoustic::analyze_album_with_progress(
            &folder,
            &paths,
            &options,
            threads,
            show_progress,
        );
        failed_files += report
            .files
            .iter()
            .filter(|file| file.error.is_some())
            .count();
        for file in &report.files {
            show_result(file);
        }
        let json = fc::report::build_json(&report)?;
        let imported = analysis::parse_report(&json)?;
        let attachment = analysis::merge_into(&mut catalog, imported.files, now_seconds());
        store::save(&catalog, &catalog_file)?;

        if args.has("json") {
            let destination = report_path(&folder);
            if let Err(error) = fc::report::write_json(&destination, &report) {
                failed_reports.push((destination, error.to_string()));
            }
        }

        completed += 1;
        println!("  {} — {} track(s)", folder.display(), paths.len());
        if attachment.stale > 0 {
            eprintln!(
                "  {} result(s) changed while being analyzed",
                attachment.stale
            );
        }
    }

    println!("  {completed} album folder(s) analyzed");
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

#[cfg(test)]
#[path = "analyze_tests.rs"]
mod tests;
