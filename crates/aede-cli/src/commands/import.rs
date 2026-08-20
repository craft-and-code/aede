//! The `import` command: take in what another tool measured.
//!
//! Entirely optional. Aède reads the structure of a file; it does not decode.
//! A user who has already run FlacCompagnon over an album has answers Aède
//! cannot compute before its own decoder arrives — whether the audio still
//! matches its MD5, whether the FLAC was made from an MP3, where the spectrum
//! stops — and those answers are worth keeping.
//!
//! Imported measurements never overwrite Aède's own. They are attributed to
//! their source and stored beside them, which is what lets `doctor` say that
//! two methods disagree.
//!
//! An analysis is filed under the **path** it describes, not under a catalog
//! entry. A report may therefore be imported before the library is scanned:
//! the records wait, and the scan that brings the files in picks them up. The
//! two operations can be done in either order.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aede_core::analysis::{self, FileAnalysis};
use aede_core::model::Catalog;
use aede_core::store;

use super::{Res, data_dir};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn import(args: &Args) -> Res {
    let dir = data_dir(args);
    let catalog_file = store::catalog_path(&dir);
    // The one command that does not demand a catalog. Analysing a library and
    // then building it is a legitimate order, and refusing the import until a
    // scan has run would force the other one for no reason.
    let mut catalog = store::load(&catalog_file)?.unwrap_or_default();

    if args.has("forget") {
        return forget(&mut catalog, &catalog_file, args);
    }

    if args.positionals.is_empty() {
        return Err("give a report, or a folder holding some: aede import report.json".into());
    }

    let mut reports = Vec::new();
    for raw in &args.positionals {
        collect_reports(Path::new(raw), &mut reports)?;
    }
    if reports.is_empty() {
        return Err("no .json report found in what was given".into());
    }

    // Matching by path first; a library that moved since the report was written
    // still matches on name and size, which is very nearly unique. When neither
    // works the record is kept as it is — the folder may simply not have been
    // scanned yet.
    let known: HashMap<&str, (u64, u64)> = catalog
        .files
        .iter()
        .map(|f| (f.path.as_str(), (f.size, f.mtime)))
        .collect();
    let mut by_name_size: HashMap<(&str, u64), Vec<&str>> = HashMap::new();
    for file in &catalog.files {
        by_name_size
            .entry((file.file_name(), file.size))
            .or_default()
            .push(file.path.as_str());
    }

    let now = now_seconds();
    let (mut matched, mut moved, mut waiting, mut stale) = (0usize, 0usize, 0usize, 0usize);
    let mut imported: Vec<FileAnalysis> = Vec::new();
    let mut pending: Vec<String> = Vec::new();

    for report_path in &reports {
        let report = match analysis::read_report(report_path) {
            Ok(report) => report,
            Err(error) => {
                return Err(format!("{}: {error}", report_path.display()).into());
            }
        };
        for mut record in report.files {
            match known.get(record.path.as_str()) {
                Some(&(size, mtime)) => {
                    // An analysis of bytes that have changed since is not an
                    // analysis of this file: keeping it would answer
                    // confidently about music that is no longer there.
                    if !record.still_applies(size, mtime) {
                        stale += 1;
                        continue;
                    }
                    matched += 1;
                }
                None => match by_name_size
                    .get(&(record.file_name(), record.size_bytes))
                    .and_then(|paths| paths.first())
                {
                    // The file moved. Filing the record under where it is now
                    // is what makes it attach again on the next scan too.
                    Some(&path) => {
                        record.path = path.to_string();
                        moved += 1;
                    }
                    None => {
                        waiting += 1;
                        if pending.len() < 10 {
                            pending.push(record.path.clone());
                        }
                    }
                },
            }
            record.imported_at = now;
            imported.push(record);
        }
    }

    // One analysis per path and per source: importing the same report twice
    // replaces, it does not accumulate.
    for record in imported {
        catalog
            .analyses
            .retain(|a| !(a.path == record.path && a.source == record.source));
        catalog.analyses.push(record);
    }
    catalog
        .analyses
        .sort_by(|a, b| (&a.path, &a.source).cmp(&(&b.path, &b.source)));
    store::save(&catalog, &catalog_file)?;

    println!("{}", ui::section("Import"));
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Reports read".into(), reports.len().to_string()]);
    table.push(vec!["Files matched".into(), matched.to_string()]);
    if moved > 0 {
        table.push(vec!["Matched by name and size".into(), moved.to_string()]);
    }
    if stale > 0 {
        table.push(vec!["Changed since the report".into(), stale.to_string()]);
    }
    if waiting > 0 {
        table.push(vec!["Waiting for a scan".into(), waiting.to_string()]);
    }
    table.push(vec![
        "Analyses stored".into(),
        catalog.analyses.len().to_string(),
    ]);
    print!("{}", table.render());

    if !pending.is_empty() {
        println!("{}", ui::section("Waiting for a scan"));
        let mut t = Table::new(&["File"]).path_limit(0, 70);
        for path in &pending {
            t.push(vec![path.clone()]);
        }
        print!("{}", t.render());
        if waiting > pending.len() {
            println!(
                "{}",
                ui::dim(&format!("  … and {} more", waiting - pending.len()))
            );
        }
        // Kept rather than refused: these are measurements of real files, and
        // the usual reason they match nothing is that their folder has not
        // been scanned yet.
        println!(
            "  {}",
            ui::dim("they are stored; scan the folders they name and they attach themselves")
        );
    }
    if stale > 0 {
        println!(
            "  {}",
            ui::yellow("some files changed after they were analysed: run FlacCompagnon again")
        );
    }
    Ok(())
}

/// Removes imported analyses, all of them or only one source's.
fn forget(catalog: &mut Catalog, catalog_file: &Path, args: &Args) -> Res {
    let before = catalog.analyses.len();
    match args.value("source") {
        Some(source) => catalog.analyses.retain(|a| a.source != source),
        None => catalog.analyses.clear(),
    }
    let removed = before - catalog.analyses.len();
    store::save(catalog, catalog_file)?;
    println!("{}", ui::section("Import"));
    println!(
        "  {} removed, {} left",
        ui::plural(removed, "analysis"),
        catalog.analyses.len()
    );
    Ok(())
}

/// Gathers the reports named, walking a folder and everything under it.
///
/// Recursive, because reports are kept the way the albums they describe are:
/// one folder per artist, one per album. A walk that only looked at the top
/// level would find nothing in the folder a user actually points at.
fn collect_reports(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    if !path.is_dir() {
        return Err(format!("\"{}\" does not exist", path.display()).into());
    }
    let found = walk_for_reports(path);
    if found.is_empty() {
        return Err(format!("no .json report under \"{}\"", path.display()).into());
    }
    out.extend(found);
    Ok(())
}

/// Every `.json` file under a folder, sorted.
///
/// Sorted so that importing the same folder twice does the same thing twice:
/// when two reports describe the same file, the same one wins both times.
fn walk_for_reports(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            // An unreadable folder must not stop the rest of the walk.
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() && name.to_ascii_lowercase().ends_with(".json") => {
                    found.push(path);
                }
                _ => {}
            }
        }
    }
    found.sort();
    found
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
