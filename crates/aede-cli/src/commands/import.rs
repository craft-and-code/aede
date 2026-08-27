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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aede_core::analysis::{self, FileAnalysis};
use aede_core::clock::now_seconds;
use aede_core::model::Catalog;
use aede_core::store;

use super::{Res, announce_window, data_dir};
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

    if args.has("pending") {
        return list_pending(&catalog, args);
    }

    if args.has("list") {
        return list_all(&catalog, args);
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

    // The matching itself lives in the core, because the scan does exactly the
    // same thing with the reports it walks over. Two copies of "which file is
    // this record about" would drift.
    let mut records: Vec<FileAnalysis> = Vec::new();
    for report_path in &reports {
        let report = match analysis::read_report(report_path) {
            Ok(report) => report,
            Err(error) => {
                return Err(format!("{}: {error}", report_path.display()).into());
            }
        };
        records.extend(report.files);
    }

    let outcome = analysis::merge_into(&mut catalog, records, now_seconds());
    store::save(&catalog, &catalog_file)?;

    println!("{}", ui::section("Import"));
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Reports read".into(), reports.len().to_string()]);
    table.push(vec!["Files matched".into(), outcome.matched.to_string()]);
    if outcome.moved > 0 {
        table.push(vec![
            "Matched by name and size".into(),
            outcome.moved.to_string(),
        ]);
    }
    if outcome.stale > 0 {
        table.push(vec![
            "Changed since the report".into(),
            outcome.stale.to_string(),
        ]);
    }
    if outcome.waiting > 0 {
        table.push(vec![
            "Waiting for a scan".into(),
            outcome.waiting.to_string(),
        ]);
    }
    table.push(vec![
        "Analyses stored".into(),
        catalog.analyses.len().to_string(),
    ]);
    print!("{}", table.render());

    if !outcome.waiting_folders.is_empty() {
        // Folders, written out whole — the same reasoning as `--pending`, and
        // the same bug before it: a path cut to a column width loses its head,
        // which is the half that says *which* folder. See [`list_pending`].
        println!("{}", ui::section("Waiting for a scan"));
        let shown: usize = outcome.waiting_folders.values().sum();
        let mut t = Table::new(&["Folder", "Analyses"]).align(1, Align::Right);
        for (folder, count) in &outcome.waiting_folders {
            t.push(vec![folder.clone(), count.to_string()]);
        }
        print!("{}", t.render());
        if outcome.waiting > shown {
            println!(
                "  {}",
                ui::dim(&format!(
                    "… and {} elsewhere — aede import --pending lists every folder",
                    ui::plural(outcome.waiting - shown, "analysis")
                ))
            );
        }
        // Kept rather than refused: these are measurements of real files, and
        // the usual reason they match nothing is that their folder has not
        // been scanned yet.
        println!(
            "  {}",
            ui::dim("they are stored; scan the folders above and they attach themselves")
        );
    }
    if outcome.stale > 0 {
        println!(
            "  {}",
            ui::yellow("some files changed after they were analysed: run FlacCompagnon again")
        );
    }
    Ok(())
}

/// `true` when a waiting record is one the user asked about.
///
/// The folders are compared as **strings**, never canonicalized: a waiting
/// record is precisely one whose path the catalog does not hold, and the
/// commonest reason is that the folder is not there any more. Asking the
/// filesystem to resolve it — as `check` legitimately does for folders it is
/// about to read — would refuse the one scope that matters here.
fn selected(record: &FileAnalysis, source: Option<&str>, folders: &[String]) -> bool {
    if source.is_some_and(|s| record.source != s) {
        return false;
    }
    folders.is_empty()
        || folders
            .iter()
            .any(|folder| aede_core::text::is_under(&record.path, folder))
}

/// Lists the imported analyses that describe no file the catalog holds,
/// **grouped by the folder they name**.
///
/// `doctor` only ever says how many are waiting, which answers "how many" and
/// not "which" — and "which" is the whole question, because a count cannot
/// tell "not scanned yet" from "will never match".
///
/// Grouped, and with the folder written out in full, for two reasons that both
/// came from the listing that did neither. One report of a hundred and forty
/// tracks is *one* decision — scan that folder, or drop it — and a hundred and
/// forty rows bury it. And a path cut to fit a column is cut at the wrong end:
/// `…/1980 Blizzard of Ozz/01 I Don't Know.flac` hides the only part that
/// distinguishes a drive that is merely unplugged from a folder that was
/// renamed. The file name identifies a file, but here nobody is looking for a
/// file — they are looking for the folder to scan, and that is the head of the
/// path, not its tail. So the column is left unbounded: this is the one
/// listing whose whole content is the path.
fn list_pending(catalog: &Catalog, args: &Args) -> Res {
    let source = args.value("source");
    let folders = scope(args);
    let pending: Vec<&FileAnalysis> = catalog
        .pending_analyses_list()
        .into_iter()
        .filter(|a| selected(a, source, &folders))
        .collect();

    println!("{}", ui::section("Waiting for a scan"));
    if pending.is_empty() {
        // Narrowed to nothing and holding nothing are different answers, and
        // "nothing is waiting" said under a filter is the wrong one: it reads
        // as a clean catalog when it may only mean the filter missed.
        let narrowed = source.is_some() || !folders.is_empty();
        println!(
            "  {}",
            match narrowed {
                true => ui::yellow("nothing waiting matches that"),
                false => ui::green("nothing is waiting"),
            }
        );
        return Ok(());
    }

    // One row per folder and per source: the unit a user acts on. Sorted by
    // folder, so the same catalog lists the same way twice.
    let mut by_folder: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for record in &pending {
        *by_folder
            .entry((
                aede_core::text::folder(&record.path),
                record.source.as_str(),
            ))
            .or_insert(0) += 1;
    }

    let window = args.window(25)?;
    let mut t = Table::new(&["Folder", "Analyses", "Source"]).align(1, Align::Right);
    for ((folder, source), count) in by_folder.iter().skip(window.offset).take(window.limit) {
        t.push(vec![
            folder.to_string(),
            count.to_string(),
            (*source).to_string(),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, by_folder.len(), "folder");
    println!(
        "  {}",
        ui::dim(&format!(
            "{} in all",
            ui::plural(pending.len(), "waiting analysis")
        ))
    );
    println!(
        "  {}",
        ui::dim("scan a folder to attach its analyses, or drop one that is gone for good:")
    );
    println!("  {}", ui::dim("aede import --forget --pending <folder>"));
    Ok(())
}

/// Folders the run is restricted to, exactly as they were typed.
fn scope(args: &Args) -> Vec<String> {
    args.positionals.clone()
}

/// What became of the analyses that name one folder.
#[derive(Default)]
struct Fate {
    /// Attached to a file the catalog holds, and still describing its bytes.
    attached: usize,
    /// Attached to a file that has changed since the report was written.
    stale: usize,
    /// Naming a path the catalog does not hold.
    waiting: usize,
}

impl Fate {
    /// `21 attached`, `19 attached, 2 stale`, `4 waiting`.
    fn wording(&self) -> String {
        let mut parts = Vec::new();
        for (count, word) in [
            (self.attached, "attached"),
            (self.stale, "stale"),
            (self.waiting, "waiting"),
        ] {
            if count > 0 {
                parts.push(format!("{count} {word}"));
            }
        }
        parts.join(", ")
    }
}

/// Lists every analysis held, grouped by the folder it names, and says what
/// became of each group.
///
/// The counterpart of `--pending`, and it was missing: the catalog could say
/// what had *failed* to attach and nothing at all about what had succeeded. So
/// a report imported over an artist whose files are clean produced 311 records,
/// no waiting line, no `doctor` entry — every symptom of having done nothing —
/// and the only way to see otherwise was to open a track page and hope to land
/// on a file the report covered. **A store that can only show its failures
/// cannot be trusted about its successes**, which is the whole reason to look.
///
/// Three fates rather than two: attached, waiting, and *stale* — attached to a
/// file whose bytes have changed since. The third is invisible everywhere else
/// and is the one that silently voids a verdict.
fn list_all(catalog: &Catalog, args: &Args) -> Res {
    let source = args.value("source");
    let folders = scope(args);
    let held: BTreeMap<&str, &aede_core::model::AudioFile> =
        catalog.files.iter().map(|f| (f.path.as_str(), f)).collect();

    let mut by_folder: BTreeMap<(&str, &str), Fate> = BTreeMap::new();
    let mut total = Fate::default();
    for record in catalog
        .analyses
        .iter()
        .filter(|a| selected(a, source, &folders))
    {
        let fate = by_folder
            .entry((
                aede_core::text::folder(&record.path),
                record.source.as_str(),
            ))
            .or_default();
        let (here, whole) = match held.get(record.path.as_str()) {
            None => (&mut fate.waiting, &mut total.waiting),
            Some(file) if record.still_applies(file.size, file.mtime) => {
                (&mut fate.attached, &mut total.attached)
            }
            Some(_) => (&mut fate.stale, &mut total.stale),
        };
        *here += 1;
        *whole += 1;
    }

    println!("{}", ui::section("Imported analyses"));
    if by_folder.is_empty() {
        // Holding nothing and being narrowed to nothing are different answers.
        let narrowed = source.is_some() || !folders.is_empty();
        println!(
            "  {}",
            match narrowed {
                true => ui::yellow("nothing imported matches that"),
                false => ui::dim("nothing imported — aede import <report.json>"),
            }
        );
        return Ok(());
    }

    let window = args.window(25)?;
    let mut t = Table::new(&["Folder", "Analyses", "State", "Source"]).align(1, Align::Right);
    for ((folder, source), fate) in by_folder.iter().skip(window.offset).take(window.limit) {
        t.push(vec![
            folder.to_string(),
            (fate.attached + fate.stale + fate.waiting).to_string(),
            fate.wording(),
            (*source).to_string(),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, by_folder.len(), "folder");
    println!("  {}", ui::dim(&format!("in all: {}", total.wording())));
    println!(
        "  {}",
        ui::dim("what one of them holds: aede track \"<title>\"")
    );
    Ok(())
}

/// Removes imported analyses: all of them, one source's, only the ones still
/// waiting for a scan, or any narrowing of those by folder.
///
/// `--pending` is what makes this safe to reach for. A plain `--forget` cannot
/// drop a report that will never attach without also losing every analysis
/// that *did* match a file — an hour of somebody else's decoding, and the one
/// thing in the catalog a re-scan cannot rebuild.
fn forget(catalog: &mut Catalog, catalog_file: &Path, args: &Args) -> Res {
    let source = args.value("source");
    let pending_only = args.has("pending");
    let folders = scope(args);

    // A folder narrows *which* records are dropped, and only `--pending` gives
    // it a meaning here — without it, `aede import --forget <folder>` reads
    // like "import that folder and forget it", which is not a thing. Refused
    // rather than ignored: a swallowed argument on a destructive command is
    // the worst place for one.
    if !folders.is_empty() && !pending_only {
        return Err(format!(
            "--forget takes no folder: \"{}\" was ignored.\n\
             To drop only what is waiting under it: aede import --forget --pending {}",
            folders.join(" "),
            folders[0]
        )
        .into());
    }

    let before = catalog.analyses.len();
    // Collected before the retain: a record only knows it is pending by
    // comparison with the files the catalog holds right now, and that
    // comparison cannot be made from inside a closure that is, at the same
    // time, mutating the very list being compared against.
    let known: std::collections::BTreeSet<String> =
        catalog.files.iter().map(|f| f.path.clone()).collect();
    catalog.analyses.retain(|a| {
        !(selected(a, source, &folders) && (!pending_only || !known.contains(&a.path)))
    });
    let removed = before - catalog.analyses.len();
    store::save(catalog, catalog_file)?;
    println!("{}", ui::section("Import"));
    println!(
        "  {} removed, {} left",
        ui::plural(removed, "analysis"),
        catalog.analyses.len()
    );
    // Which catalog, because "311 removed" and a report that still shows them
    // is a contradiction the user cannot investigate without knowing what was
    // written. `scan` ends this way for the same reason; a command that
    // *destroys* something has more need of it, not less. `--data` and
    // `$AEDE_HOME` both move this file, and the second is easy to forget.
    println!(
        "{}",
        ui::dim(&format!("  catalog: {}", catalog_file.display()))
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
