//! Scanning folders and managing the watched folder list.

use std::error::Error;
use std::path::PathBuf;

use aede_core::model::{Catalog, Id};
use aede_core::scan::{self, Progress, ScanOptions};
use aede_core::stats;
use aede_core::store;
use aede_core::text;

use super::{Res, data_dir, load, totals};
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
    table.push(vec!["Read from disk".into(), report.read.to_string()]);
    table.push(vec![
        "Reused from previous scan".into(),
        report.reused.to_string(),
    ]);
    if report.removed > 0 {
        table.push(vec![
            "Gone since last scan".into(),
            report.removed.to_string(),
        ]);
    }
    // Only shown when a report was actually met: on a library holding none, a
    // line reading zero would answer a question nobody asked.
    if report.reports > 0 {
        table.push(vec![
            "Analyses imported".into(),
            format!(
                "{} from {}",
                report.analyses,
                ui::plural(report.reports, "report")
            ),
        ]);
    }
    if report.attached > 0 {
        table.push(vec![
            "Analyses now attached".into(),
            report.attached.to_string(),
        ]);
    }
    table.push(vec!["Elapsed".into(), ui::elapsed(report.elapsed_ms)]);
    print!("{}", table.render());

    if !report.failures.is_empty() {
        println!("{}", ui::section("Unreadable files"));
        let mut t = Table::new(&["File", "Reason"]).path_limit(0, 60);
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
        // An empty list is legitimate once a catalog exists: it means the last
        // watched folder was dropped, and this scan is what empties the
        // catalog — exactly what `roots --remove` said to run. Refusing here
        // would leave those files in the catalog with no way out.
        if stored.is_none() {
            return Err("give at least one folder: aede scan ~/Music".into());
        }
        println!(
            "{}",
            ui::yellow("No folder is watched any more: the catalog will be emptied.")
        );
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
        // Removing a folder from the list does not empty the catalog: the
        // files stay until a scan rebuilds it from the folders still watched.
        // Naming this one again on that scan would simply watch it anew.
        println!(
            "{}",
            ui::dim("  its files stay in the catalog until the next scan")
        );
        println!(
            "{}",
            ui::dim("  run `aede scan` with no folder to drop them")
        );
        return Ok(());
    }

    println!("{}", ui::section("Watched folders"));
    // The same three measures every listing and every page carries. A folder
    // count without a weight is the one figure a user cannot act on: knowing a
    // drive holds four thousand tracks says nothing about whether it will fit
    // anywhere.
    let mut t = Table::new(&["Folder", "Tracks", "Duration", "Size"])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .path_limit(0, 60);
    let mut counted: std::collections::BTreeSet<Id> = Default::default();
    for root in &catalog.roots {
        let tracks = tracks_under(&catalog, root);
        counted.extend(tracks.iter().copied());
        let (duration, size) = totals(&catalog, &tracks);
        t.push(vec![
            root.clone(),
            tracks.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
        ]);
    }

    // Files left over from a folder that was dropped and not yet rescanned.
    // The removal message promises they are still there; a table that does not
    // show them makes that promise unverifiable.
    let orphans: Vec<Id> = catalog
        .tracks
        .iter()
        .map(|t| t.id)
        .filter(|id| !counted.contains(id))
        .collect();
    if !orphans.is_empty() {
        let (duration, size) = totals(&catalog, &orphans);
        t.push(vec![
            "(no longer watched)".into(),
            orphans.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
        ]);
    }
    print!("{}", t.render());

    if catalog.roots.len() + usize::from(!orphans.is_empty()) > 1 {
        let all: Vec<Id> = catalog.tracks.iter().map(|t| t.id).collect();
        let (duration, size) = totals(&catalog, &all);
        println!(
            "  {}",
            ui::dim(&format!(
                "{} · {} · {} in all",
                ui::plural(all.len(), "track"),
                ui::long_duration(duration),
                text::format_size(size)
            ))
        );
    }
    if !orphans.is_empty() {
        println!(
            "  {}",
            ui::dim("run `aede scan` to drop what is no longer watched")
        );
    }
    // Where the answers themselves are kept. `roots` is the command someone
    // runs to ask where their things are, and the catalog file was the one
    // location it never named — only `scan` mentioned it, once, in passing.
    println!(
        "  {}",
        ui::dim(&format!("catalog: {}", catalog_file.display()))
    );
    Ok(())
}

/// Tracks whose file sits in a folder, or in something under it.
///
/// On the path boundary, never on the bare string: `/music/Rock` would
/// otherwise claim every file of `/music/Rockabilly`, and the count of a
/// watched folder would quietly include a neighbour's.
fn tracks_under(catalog: &Catalog, root: &str) -> Vec<Id> {
    catalog
        .tracks
        .iter()
        .filter(|t| {
            catalog
                .file(t.file_id)
                .is_some_and(|f| aede_core::text::is_under(&f.path, root))
        })
        .map(|t| t.id)
        .collect()
}
