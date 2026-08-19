//! The `check` command: verify that the files are still what they were.
//!
//! Deliberately opt-in. Every checksum a container carries covers the whole
//! stream, so verifying means reading every byte of the library — minutes on an
//! SSD, an hour or more on a spinning disk. That is not something a `scan`
//! should decide to do on its own.
//!
//! The verdict is stored per file and survives across scans, so the cost is
//! paid once: only files that are new, or that changed, come back without one.
//! It is also saved as the work goes, so an interrupted run keeps what it
//! established and the next one carries on where it stopped.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use aede_core::audit::integrity::{self, Verdict};
use aede_core::model::{Catalog, Id, IntegrityRecord};
use aede_core::store;
use aede_core::text;

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

/// Above this many files, the run is worth announcing before it starts.
const LONG_RUN: usize = 500;

/// Files verified between two saves.
///
/// Small enough that an interruption costs little, large enough that writing
/// the catalog stays a rounding error next to reading the audio.
const SAVE_EVERY: usize = 250;

pub fn check(args: &Args) -> Res {
    let dir = data_dir(args);
    let catalog_file = store::catalog_path(&dir);
    let mut catalog = load(args)?;

    // A folder given on the command line restricts the work to it. Verifying a
    // whole library at once is the kind of thing one wants to try on a corner
    // first.
    let scope = resolve_scope(args)?;
    let queue = to_verify(&catalog, &scope, args.has("full"));

    if queue.is_empty() {
        println!("{}", ui::section("Integrity"));
        if scope.is_empty() {
            println!("  {}", ui::dim("every file already has a verdict"));
            println!("  {}", ui::dim("aede check --full verifies them again"));
        } else if in_scope_count(&catalog, &scope) == 0 {
            println!(
                "  {}",
                ui::yellow("no file of the catalog is in that folder")
            );
        } else {
            println!("  {}", ui::dim("every file there already has a verdict"));
        }
        return Ok(());
    }

    announce(&catalog, &queue);

    let started = Instant::now();
    let total = queue.len();
    let done = AtomicUsize::new(0);
    let mut failures: Vec<(String, String)> = Vec::new();
    let now = now_seconds();
    let threads = resolve_threads(args.usize_value("threads", 0));
    let interactive = ui::is_interactive();

    // One batch at a time, each saved before the next starts: a run stopped
    // half-way keeps everything it verified.
    for batch in queue.chunks(SAVE_EVERY) {
        let pending = Mutex::new(batch.to_vec());
        let results: Mutex<Vec<(Id, IntegrityRecord)>> = Mutex::new(Vec::new());
        let batch_failures: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

        std::thread::scope(|scope| {
            for _ in 0..threads.min(batch.len()) {
                scope.spawn(|| {
                    loop {
                        let Some((id, path)) =
                            pending.lock().unwrap_or_else(|e| e.into_inner()).pop()
                        else {
                            break;
                        };
                        match integrity::check(&path) {
                            Ok(report) => {
                                let record = IntegrityRecord {
                                    verdict: report.verdict,
                                    method: report.method.to_string(),
                                    checked_at: now,
                                };
                                results
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .push((id, record));
                            }
                            // Being unreadable is not a verdict on the audio:
                            // the file keeps no verdict and is reported apart.
                            Err(error) => batch_failures
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .push((path.to_string_lossy().to_string(), error.to_string())),
                        }
                        // Redrawn every few files: often enough to look alive,
                        // rarely enough not to interleave between threads.
                        let count = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if interactive && (count.is_multiple_of(4) || count == total) {
                            print!("\r  {count}/{total}   ");
                            use std::io::Write;
                            let _ = std::io::stdout().flush();
                        }
                    }
                });
            }
        });

        for (id, record) in results.into_inner().unwrap_or_else(|e| e.into_inner()) {
            if let Some(file) = catalog.files.get_mut(id as usize) {
                file.integrity = Some(record);
            }
        }
        failures.extend(
            batch_failures
                .into_inner()
                .unwrap_or_else(|e| e.into_inner()),
        );
        store::save(&catalog, &catalog_file)?;
    }
    if interactive {
        println!("\r  {total}/{total}   ");
    }

    report(&catalog, &scope, started, &failures);
    Ok(())
}

/// Folders or files the run is restricted to, canonicalized.
///
/// Empty means the whole catalog.
fn resolve_scope(args: &Args) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut scope = Vec::new();
    for raw in &args.positionals {
        let path = Path::new(raw);
        if !path.exists() {
            return Err(format!("\"{raw}\" does not exist").into());
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        scope.push(canonical.to_string_lossy().to_string());
    }
    Ok(scope)
}

/// `true` when the path is inside one of the folders given, or is one of them.
fn is_in_scope(path: &str, scope: &[String]) -> bool {
    if scope.is_empty() {
        return true;
    }
    scope
        .iter()
        .any(|root| path == root || path.starts_with(&format!("{root}/")))
}

fn in_scope_count(catalog: &Catalog, scope: &[String]) -> usize {
    catalog
        .files
        .iter()
        .filter(|f| is_in_scope(&f.path, scope))
        .count()
}

fn to_verify(catalog: &Catalog, scope: &[String], full: bool) -> Vec<(Id, PathBuf)> {
    catalog
        .files
        .iter()
        .filter(|file| is_in_scope(&file.path, scope))
        // By default only what has no verdict yet, which is what makes a second
        // run cheap. `--full` re-reads everything, for a disk under suspicion.
        .filter(|file| full || file.integrity.is_none())
        .map(|file| (file.id, PathBuf::from(&file.path)))
        .collect()
}

/// Says what is about to happen, in volume rather than in a predicted time: how
/// long it takes depends on the disk, and a wrong estimate is worse than none.
fn announce(catalog: &Catalog, queue: &[(Id, PathBuf)]) {
    let bytes: u64 = queue
        .iter()
        .filter_map(|(id, _)| catalog.file(*id))
        .map(|f| f.size)
        .sum();
    println!(
        "{} {}",
        ui::bold("Verifying"),
        ui::dim(&format!(
            "{} to read, {}",
            ui::plural(queue.len(), "file"),
            text::format_size(bytes)
        ))
    );
    if queue.len() >= LONG_RUN {
        println!(
            "  {}",
            ui::yellow("this reads every byte: minutes on an SSD, longer on a mechanical disk")
        );
        println!(
            "  {}",
            ui::dim("stopping it is safe — verified files are saved as the run goes")
        );
    }
}

fn report(catalog: &Catalog, scope: &[String], started: Instant, failures: &[(String, String)]) {
    let (mut intact, mut damaged, mut nothing, mut unchecked) = (0usize, 0usize, 0usize, 0usize);
    for file in catalog.files.iter().filter(|f| is_in_scope(&f.path, scope)) {
        match file.integrity.as_ref().map(|r| &r.verdict) {
            Some(Verdict::Intact) => intact += 1,
            Some(Verdict::Damaged { .. }) => damaged += 1,
            Some(Verdict::NothingToCheck) => nothing += 1,
            None => unchecked += 1,
        }
    }

    println!("{}", ui::section("Integrity"));
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Intact".into(), intact.to_string()]);
    table.push(vec!["Damaged".into(), damaged.to_string()]);
    table.push(vec!["No checksum in the file".into(), nothing.to_string()]);
    if unchecked > 0 {
        table.push(vec!["Not verified".into(), unchecked.to_string()]);
    }
    table.push(vec![
        "Elapsed".into(),
        ui::elapsed(started.elapsed().as_millis()),
    ]);
    print!("{}", table.render());
    if !scope.is_empty() {
        println!("  {}", ui::dim(&format!("in {}", scope.join(", "))));
    }

    if damaged > 0 {
        println!("{}", ui::section("Damaged files"));
        let mut t = Table::new(&["File", "Problem"]).path_limit(0, 60);
        for file in catalog.files.iter().filter(|f| is_in_scope(&f.path, scope)) {
            if let Some(record) = &file.integrity
                && let Verdict::Damaged { detail } = &record.verdict
            {
                t.push(vec![file.path.clone(), detail.clone()]);
            }
        }
        print!("{}", t.render());
        println!(
            "{}",
            ui::dim(
                "  a damaged file cannot be repaired: restore it from a backup or rip it again"
            )
        );
    }

    if !failures.is_empty() {
        println!("{}", ui::section("Unreadable files"));
        let mut t = Table::new(&["File", "Reason"]).path_limit(0, 60);
        for (path, reason) in failures.iter().take(20) {
            t.push(vec![path.clone(), reason.clone()]);
        }
        print!("{}", t.render());
    }
}

fn resolve_threads(requested: usize) -> usize {
    if requested > 0 {
        return requested;
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
