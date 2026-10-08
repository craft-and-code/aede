//! The `playlist` command: an `.m3u` in every album folder.
//!
//! Same shape as `spectrum`, and for the same reason: the catalog already
//! knows what an album is, which tracks it holds and in what order, and a
//! second answer read off the filesystem would drift from the first. The
//! folders on the command line narrow it; they do not replace it.

#[cfg(test)]
use std::path::PathBuf;

use aede_core::playlist::{self, Style};

use super::{Res, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn playlist(args: &Args) -> Res {
    let catalog = load(args)?;
    let scope = super::scope_of(args, &catalog)?;
    let style = match args.has("simple") {
        true => Style::Simple,
        false => Style::Extended,
    };

    let mut folders = albums(&catalog, &scope);
    if args.has("artists") {
        for (folder, tracks) in discographies(&catalog, &scope) {
            folders.entry(folder).or_default().extend(tracks);
        }
    }
    let wanted = render_folders(&catalog, folders, style)?;

    println!("{}", ui::section("Playlists"));
    if wanted.is_empty() {
        println!(
            "  {}",
            ui::yellow("no album of the catalog is in that folder")
        );
        return Ok(());
    }

    // Split before writing anything, so the count on screen is the work and
    // not the library — and so a --dry-run says the same thing the real run
    // will do rather than something close to it.
    let (to_write, current): (Vec<_>, Vec<_>) = wanted
        .into_iter()
        .partition(|(path, text)| !playlist::already_says(path, text));

    if args.has("dry-run") {
        let mut t = Table::new(&["Playlist", "Tracks"]).align(1, Align::Right);
        for (path, text) in to_write.iter().take(20) {
            t.push(vec![path.display().to_string(), lines(text).to_string()]);
        }
        print!("{}", t.render());
        if to_write.len() > 20 {
            println!(
                "{}",
                ui::dim(&format!("  … and {} more", to_write.len() - 20))
            );
        }
        summarise(to_write.len(), 0, current.len(), &[]);
        return Ok(());
    }

    let mut written = 0usize;
    let mut failures: Vec<(String, String)> = Vec::new();
    for (path, text) in &to_write {
        match playlist::write_atomic(path, text) {
            Ok(()) => written += 1,
            Err(e) => failures.push((path.display().to_string(), e.to_string())),
        }
    }
    summarise(0, written, current.len(), &failures);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("{} playlist(s) could not be written", failures.len()).into())
    }
}

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;

fn summarise(planned: usize, written: usize, current: usize, failures: &[(String, String)]) {
    let mut t = Table::plain(2).align(1, Align::Right);
    if planned > 0 {
        t.push(vec!["To write".into(), planned.to_string()]);
    }
    if written > 0 || planned == 0 {
        t.push(vec!["Written".into(), written.to_string()]);
    }
    // Said even when it is the whole answer: a second run over an unchanged
    // library writes nothing, and an empty screen there reads as a failure
    // rather than as "there was nothing to do".
    t.push(vec!["Already up to date".into(), current.to_string()]);
    if !failures.is_empty() {
        t.push(vec!["Failed".into(), failures.len().to_string()]);
    }
    print!("{}", t.render());
    if failures.is_empty() {
        return;
    }
    super::print_failure_table("What could not be written", "Playlist", failures);
}

/// Tracks in a rendered playlist, for the dry run's column.
fn lines(text: &str) -> usize {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .count()
}

use aede_core::playlist::folders::{albums, discographies, render_folders};
