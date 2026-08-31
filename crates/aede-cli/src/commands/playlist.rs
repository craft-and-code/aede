//! The `playlist` command: an `.m3u` in every album folder.
//!
//! Same shape as `spectrum`, and for the same reason: the catalog already
//! knows what an album is, which tracks it holds and in what order, and a
//! second answer read off the filesystem would drift from the first. The
//! folders on the command line narrow it; they do not replace it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aede_core::model::{Catalog, Id};
use aede_core::playlist::{self, Style};

use super::{Res, load};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn playlist(args: &Args) -> Res {
    let catalog = load(args)?;
    let scope = super::scope_of(args)?;
    let style = match args.has("simple") {
        true => Style::Simple,
        false => Style::Extended,
    };

    let mut wanted: Vec<(PathBuf, String)> = albums(&catalog, &scope, style);
    if args.has("artists") {
        wanted.extend(discographies(&catalog, &scope, style));
    }

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
        match std::fs::write(path, text) {
            Ok(()) => written += 1,
            Err(e) => failures.push((path.display().to_string(), e.to_string())),
        }
    }
    summarise(0, written, current.len(), &failures);
    Ok(())
}

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
    println!("{}", ui::section("What could not be written"));
    let mut t = Table::new(&["Playlist", "Reason"]).path_limit(0, 60);
    for (path, reason) in failures.iter().take(20) {
        t.push(vec![path.clone(), reason.clone()]);
    }
    print!("{}", t.render());
}

/// Tracks in a rendered playlist, for the dry run's column.
fn lines(text: &str) -> usize {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .count()
}

/// One playlist per album folder, holding that album in its own order.
fn albums(catalog: &Catalog, scope: &[String], style: Style) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for release in &catalog.releases {
        if !super::in_scope(&release.folder, scope) {
            continue;
        }
        // A box set laid out as `Album/Disc 1`, `Album/Disc 2` is *one*
        // release living in the parent, so its playlist goes there and spans
        // the discs — which is exactly the file somebody wants when the tracks
        // are numbered 1..17 twice over.
        let folder = PathBuf::from(&release.folder);
        let text = playlist::render(catalog, &release.track_ids, Some(&folder), style);
        if lines(&text) == 0 {
            continue;
        }
        out.push((folder.join(playlist::file_name(&folder)), text));
    }
    out
}

/// One playlist per artist folder, holding every album of that artist in order.
///
/// The artist folder is not a thing the catalog holds — it is inferred as the
/// folder every one of that artist's albums sits in. Where they do not share
/// one, or share only a watched root, nothing is written: a library laid out
/// flat would otherwise get one playlist per artist dumped in its root, which
/// is not tidying but littering.
fn discographies(catalog: &Catalog, scope: &[String], style: Style) -> Vec<(PathBuf, String)> {
    let mut by_artist: BTreeMap<Id, Vec<&aede_core::model::Release>> = BTreeMap::new();
    for release in &catalog.releases {
        if let Some(artist) = release.album_artist_id
            && super::in_scope(&release.folder, scope)
        {
            by_artist.entry(artist).or_default().push(release);
        }
    }

    let roots: Vec<&str> = catalog.roots.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for (_, mut releases) in by_artist {
        let Some(folder) = shared_folder(&releases) else {
            continue;
        };
        let name = folder.to_string_lossy().to_string();
        // The root of a library is not an artist folder, and neither is a
        // folder that *is* one of the album folders — that one already has its
        // own playlist under the very same name.
        if roots.contains(&name.as_str()) || releases.iter().any(|r| r.folder == name) {
            continue;
        }
        // Chronological, which is the order a discography is read in; the
        // title breaks a tie so that two records of the same year come out the
        // same way on every machine.
        releases.sort_by(|a, b| {
            a.year
                .unwrap_or(0)
                .cmp(&b.year.unwrap_or(0))
                .then_with(|| a.title.cmp(&b.title))
        });
        let tracks: Vec<Id> = releases.iter().flat_map(|r| r.track_ids.clone()).collect();
        let text = playlist::render(catalog, &tracks, Some(&folder), style);
        if lines(&text) == 0 {
            continue;
        }
        out.push((folder.join(playlist::file_name(&folder)), text));
    }
    out
}

/// The deepest folder holding every one of these releases, or `None`.
fn shared_folder(releases: &[&aede_core::model::Release]) -> Option<PathBuf> {
    let mut shared: Option<PathBuf> = None;
    for release in releases {
        let parent = Path::new(&release.folder).parent()?.to_path_buf();
        shared = Some(match shared {
            None => parent,
            Some(so_far) if so_far == parent => so_far,
            // Albums under different parents: no single artist folder to speak
            // of, and inventing one would put the file somewhere arbitrary.
            Some(_) => return None,
        });
    }
    shared
}
