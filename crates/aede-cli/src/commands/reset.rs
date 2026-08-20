//! The `reset` command: throw the catalog away.
//!
//! Most of what is lost comes back with one `aede scan`, which is why this is a
//! small command. Three things do not come back: the **watched folders**, the
//! **integrity verdicts**, which may have cost an hour of reading, and the
//! **imported analyses**, which cost a run of another program entirely. So the
//! confirmation says what is at stake rather than asking a bare "are you sure",
//! and the command prints the scan that rebuilds what it removed.

use std::io::{IsTerminal, Write};

use aede_core::store;
use aede_core::text;

use super::{Res, data_dir, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn reset(args: &Args) -> Res {
    let dir = data_dir(args);
    let catalog_file = store::catalog_path(&dir);
    if !catalog_file.exists() {
        println!("{}", ui::section("Reset"));
        println!("  {}", ui::dim("there is no catalog to remove"));
        return Ok(());
    }

    let catalog = load(args)?;
    let size = std::fs::metadata(&catalog_file)
        .map(|m| m.len())
        .unwrap_or(0);
    let verified = catalog
        .files
        .iter()
        .filter(|f| f.integrity.is_some())
        .count();

    println!("{}", ui::section("About to remove the catalog"));
    let mut table = Table::plain(2).align(1, Align::Right);
    table.push(vec!["Tracks".into(), catalog.tracks.len().to_string()]);
    table.push(vec!["Albums".into(), catalog.releases.len().to_string()]);
    table.push(vec!["Artists".into(), catalog.artists.len().to_string()]);
    table.push(vec![
        "Watched folders".into(),
        catalog.roots.len().to_string(),
    ]);
    table.push(vec!["Integrity verdicts".into(), verified.to_string()]);
    if !catalog.analyses.is_empty() {
        table.push(vec![
            "Imported analyses".into(),
            catalog.analyses.len().to_string(),
        ]);
    }
    table.push(vec!["File".into(), text::format_size(size)]);
    print!("{}", table.render());
    println!("  {}", ui::dim(&catalog_file.display().to_string()));

    // What a scan brings back and what it does not: the difference is the whole
    // reason to hesitate.
    println!(
        "  {}",
        ui::dim("a scan rebuilds the catalog; the watched folders and the integrity")
    );
    println!("  {}", ui::dim("verdicts are lost and have to be redone"));
    if !catalog.analyses.is_empty() {
        println!(
            "  {}",
            ui::dim("the imported analyses go too, and have to be imported again")
        );
    }

    if !confirmed(args)? {
        println!("  {}", ui::green("nothing was removed"));
        return Ok(());
    }

    // The folders are printed after the deletion, not before: at that point
    // they are the only way back, and they have to be readable on screen.
    let roots = catalog.roots.clone();
    std::fs::remove_file(&catalog_file)?;
    println!("{} catalog removed", ui::green("→"));
    if !roots.is_empty() {
        let quoted: Vec<String> = roots.iter().map(|r| format!("\"{r}\"")).collect();
        println!("  {}", ui::dim("to rebuild it:"));
        println!("  aede scan {}", quoted.join(" "));
    }
    Ok(())
}

/// Asks, unless `--yes` was given.
///
/// With no terminal to ask — a script, a pipe — the command refuses rather than
/// assuming an answer. Assuming "no" would make a scripted reset fail silently;
/// assuming "yes" would delete a catalog nobody agreed to lose.
fn confirmed(args: &Args) -> Result<bool, Box<dyn std::error::Error>> {
    if args.has("yes") {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err("no terminal to confirm on: add --yes to remove the catalog".into());
    }
    print!("  Type \"yes\" to confirm: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("yes"))
}
