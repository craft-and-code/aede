//! Scanning folders and managing the watched folder list.

use std::error::Error;
use std::path::PathBuf;

use aede_core::model::Catalog;
use aede_core::scan::{self, Progress, ScanOptions};
use aede_core::stats;
use aede_core::store;

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn scan(args: &Args) -> Res {
    let dir = data_dir(args);
    let catalog_file = store::catalog_path(&dir);

    // A catalog from an earlier version must not block the scan: we start over
    // and say so.
    let stored = match store::load(&catalog_file) {
        Ok(value) => value,
        Err(store::StoreError::Version { .. }) => {
            println!(
                "{}",
                ui::yellow("Catalog from an earlier version: full scan.")
            );
            None
        }
        Err(e) => return Err(e.into()),
    };

    let roots = resolve_roots(args, stored.as_ref())?;
    // `--full` only disables the tag cache; the watched folders are kept.
    let previous = if args.has("full") {
        None
    } else {
        stored.as_ref()
    };

    let options = ScanOptions {
        threads: args.usize_value("threads", 0),
        follow_symlinks: args.has("follow-symlinks"),
        skip_hidden: !args.has("include-hidden"),
    };

    println!("{}", ui::bold("Scanning folders…"));
    let mut discovered = 0usize;
    let (catalog, report) = scan::scan(&roots, previous, &options, |progress| match progress {
        Progress::Discovered(count) => {
            discovered = count;
            println!("  {count} audio files spotted");
        }
        Progress::Read { done, total } => {
            if total > 0 {
                print!("\r  reading: {done}/{total}   ");
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
        }
    })?;
    if report.read > 0 {
        println!();
    }

    store::save(&catalog, &catalog_file)?;

    println!("{}", ui::section("Scan complete"));
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Files found".into(), report.found.to_string()]);
    table.push(vec!["Read".into(), report.read.to_string()]);
    table.push(vec![
        "Reused from previous scan".into(),
        report.reused.to_string(),
    ]);
    if report.removed > 0 {
        table.push(vec!["Gone since".into(), report.removed.to_string()]);
    }
    table.push(vec!["Elapsed".into(), format!("{} ms", report.elapsed_ms)]);
    print!("{}", table.render());

    if !report.failures.is_empty() {
        println!("{}", ui::section("Unreadable files"));
        let mut t = Table::new(&["File", "Reason"]).limit(0, 60);
        for (path, reason) in report.failures.iter().take(20) {
            t.push(vec![path.clone(), reason.clone()]);
        }
        print!("{}", t.render());
        if report.failures.len() > 20 {
            println!(
                "{}",
                ui::dim(&format!("  … and {} more", report.failures.len() - 20))
            );
        }
    }

    let s = stats::compute(&catalog);
    println!(
        "\n{} {} · {} · {} · {}",
        ui::green("→"),
        ui::plural(s.tracks, "track"),
        ui::plural(s.releases, "album"),
        ui::plural(s.artists, "artist"),
        ui::long_duration(s.total_duration_ms)
    );
    println!(
        "{}",
        ui::dim(&format!("  catalog: {}", catalog_file.display()))
    );
    Ok(())
}

/// Works out which folders to walk.
///
/// The watched folders accumulate across runs. Scanning a second library must
/// add to the catalog, not silently replace it — that mistake costs the user
/// their whole catalog with no warning. `--replace` asks for the old
/// behaviour explicitly.
fn resolve_roots(args: &Args, stored: Option<&Catalog>) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if !args.has("replace")
        && let Some(catalog) = stored
    {
        for root in &catalog.roots {
            let path = PathBuf::from(root);
            if !path.is_dir() {
                return Err(format!(
                    "watched folder \"{root}\" is unreachable.\n\
                         Plug the drive back in, or drop it with: aede roots --remove \"{root}\"",
                )
                .into());
            }
            roots.push(path);
        }
    }

    for raw in &args.positionals {
        let path = PathBuf::from(raw);
        if !path.is_dir() {
            return Err(format!("\"{raw}\" is not a readable folder").into());
        }
        roots.push(std::fs::canonicalize(&path).unwrap_or(path));
    }

    if roots.is_empty() {
        return Err("give at least one folder: aede scan ~/Music".into());
    }

    Ok(dedupe_roots(roots))
}

/// Removes duplicates and folders already covered by another root, so a file
/// is never walked twice.
fn dedupe_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if kept.iter().any(|k| root.starts_with(k)) {
            continue;
        }
        kept.retain(|k| !k.starts_with(&root));
        kept.push(root);
    }
    kept
}

/// Lists the watched folders, or drops one from the list.
pub fn roots(args: &Args) -> Res {
    let dir = data_dir(args);
    let catalog_file = store::catalog_path(&dir);
    let mut catalog = load(args)?;

    if let Some(target) = args.value("remove") {
        let wanted = std::fs::canonicalize(target)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| target.to_string());
        let before = catalog.roots.len();
        catalog.roots.retain(|r| r != &wanted && r != target);
        if catalog.roots.len() == before {
            return Err(format!("\"{target}\" is not a watched folder").into());
        }
        store::save(&catalog, &catalog_file)?;
        println!("{} no longer watching {target}", ui::green("->"));
        println!(
            "{}",
            ui::dim("  run `aede scan` to drop its files from the catalog")
        );
        return Ok(());
    }

    println!("{}", ui::section("Watched folders"));
    let mut t = Table::new(&["Folder", "Tracks"]).align(1, Align::Right);
    for root in &catalog.roots {
        let count = catalog
            .files
            .iter()
            .filter(|f| f.path.starts_with(root.as_str()))
            .count();
        t.push(vec![root.clone(), count.to_string()]);
    }
    print!("{}", t.render());
    Ok(())
}
