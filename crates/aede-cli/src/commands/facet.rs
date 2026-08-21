//! The `genre` and `label` commands: one page per facet.
//!
//! A genre and a label are entities of the model like any other, with their own
//! rows and their own links — but until now the only way to reach them was the
//! plural listing, which counts them without ever letting you look inside one.
//! A count you cannot open is a dead end, and the interface that comes at M2
//! will want exactly this page behind every genre it displays.
//!
//! Both pages have the same shape, because the two answer the same question in
//! two vocabularies: what is in here, and who is in it. The tracks they gather
//! are a **selection**, so `--csv` and `--m3u` apply to them as they do to an
//! album or an artist.

use std::collections::BTreeMap;

use aede_core::model::{Catalog, Id, TitleMatch};
use aede_core::text;

use super::{Res, announce_limit, load, selection_output, totals};
use crate::args::Args;
use crate::ui::{self, Align, Table};

/// Rows shown before the page starts filling the terminal.
const DEFAULT_LIMIT: usize = 50;

pub fn show_genre(args: &Args) -> Res {
    let catalog = load(args)?;
    let name = args.positionals.join(" ");
    if name.trim().is_empty() {
        return Err("give a genre: aede genre metal".into());
    }

    let (found, kind) = catalog.find_genres(&name);
    if found.is_empty() {
        return Err(
            format!("no genre matches \"{name}\".\nRun \"aede genres\" for the list.").into(),
        );
    }
    // Several genres can match — "metal" reaches "Black Metal" and "Doom
    // Metal" — and the page then covers all of them rather than picking one.
    let ids: Vec<Id> = found.iter().map(|g| g.id).collect();
    let names: Vec<String> = found.iter().map(|g| g.name.clone()).collect();

    let mut tracks: Vec<Id> = Vec::new();
    for &id in &ids {
        tracks.extend(catalog.tracks_of_genre(id));
    }
    tracks.sort_unstable();
    tracks.dedup();

    if let Some(result) = selection_output(&catalog, &tracks, args) {
        return result;
    }

    let heading = names.join(", ");
    println!("{}", ui::section(&heading));
    if kind == TitleMatch::Partial {
        println!(
            "  {}",
            ui::dim(&format!(
                "no genre is called \"{name}\"; showing the ones containing it"
            ))
        );
    }
    print_totals(&catalog, &tracks);

    let releases = releases_holding(&catalog, &tracks);
    print_albums(&catalog, &releases, args);
    print_artists(&catalog, &tracks, args);
    Ok(())
}

pub fn show_label(args: &Args) -> Res {
    let catalog = load(args)?;
    let name = args.positionals.join(" ");
    if name.trim().is_empty() {
        return Err("give a label: aede label \"Blue Note\"".into());
    }

    let (found, kind) = catalog.find_labels(&name);
    if found.is_empty() {
        return Err(
            format!("no label matches \"{name}\".\nRun \"aede labels\" for the list.").into(),
        );
    }
    let ids: Vec<Id> = found.iter().map(|l| l.id).collect();
    let names: Vec<String> = found.iter().map(|l| l.name.clone()).collect();

    let mut tracks: Vec<Id> = Vec::new();
    let mut releases: Vec<Id> = Vec::new();
    for &id in &ids {
        tracks.extend(catalog.tracks_of_label(id));
        releases.extend(catalog.releases_of_label(id));
    }
    tracks.sort_unstable();
    tracks.dedup();
    releases.sort_unstable();
    releases.dedup();

    if let Some(result) = selection_output(&catalog, &tracks, args) {
        return result;
    }

    println!("{}", ui::section(&names.join(", ")));
    if kind == TitleMatch::Partial {
        println!(
            "  {}",
            ui::dim(&format!(
                "no label is called \"{name}\"; showing the ones containing it"
            ))
        );
    }
    print_totals(&catalog, &tracks);
    print_albums(&catalog, &releases, args);
    print_artists(&catalog, &tracks, args);
    Ok(())
}

/// The three measures every page carries: count, playing time, size on disk.
fn print_totals(catalog: &Catalog, tracks: &[Id]) {
    let (duration, size) = totals(catalog, tracks);
    println!(
        "  {}",
        ui::dim(&format!(
            "{} · {} · {}",
            ui::plural(tracks.len(), "track"),
            text::format_duration(duration),
            text::format_size(size)
        ))
    );
}

/// Releases the given tracks belong to, in catalog order.
fn releases_holding(catalog: &Catalog, tracks: &[Id]) -> Vec<Id> {
    let mut releases: std::collections::BTreeSet<Id> = Default::default();
    for &id in tracks {
        if let Some(release) = catalog.track(id).and_then(|t| t.release_id) {
            releases.insert(release);
        }
    }
    releases.into_iter().collect()
}

fn print_albums(catalog: &Catalog, releases: &[Id], args: &Args) {
    if releases.is_empty() {
        return;
    }
    let limit = args.usize_value("limit", DEFAULT_LIMIT);
    let mut rows: Vec<(&aede_core::model::Release, u64, u64)> = releases
        .iter()
        .filter_map(|&id| catalog.release(id))
        .map(|r| {
            let (duration, size) = totals(catalog, &r.track_ids);
            (r, duration, size)
        })
        .collect();
    rows.sort_by(|a, b| {
        a.0.year
            .unwrap_or(u32::MAX)
            .cmp(&b.0.year.unwrap_or(u32::MAX))
            .then_with(|| a.0.title.cmp(&b.0.title))
    });

    println!("{}", ui::section("Albums"));
    let mut t = Table::new(&["Year", "Album", "Artist", "Tracks", "Duration", "Size"])
        .align(3, Align::Right)
        .align(4, Align::Right)
        .align(5, Align::Right)
        .limit(1, 40)
        .limit(2, 30);
    let total = rows.len();
    for (release, duration, size) in rows.into_iter().take(limit) {
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        t.push(vec![
            release
                .year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "—".into()),
            release.title.clone(),
            artist,
            release.track_ids.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
        ]);
    }
    print!("{}", t.render());
    announce_limit(total.min(limit), total, "album");
}

/// Who is audible here, ranked by how much of it they carry.
///
/// Performing roles only: a genre is something you *hear*, and counting the
/// lyricist of a track among the artists of a style would be a different claim.
fn print_artists(catalog: &Catalog, tracks: &[Id], args: &Args) {
    let mut counts: BTreeMap<Id, usize> = BTreeMap::new();
    for &track in tracks {
        for (artist, role) in catalog.credits_on(aede_core::model::EntityKind::Track, track) {
            if aede_core::model::is_performing_role(role) {
                *counts.entry(artist.id).or_insert(0) += 1;
            }
        }
    }
    if counts.is_empty() {
        return;
    }
    let mut rows: Vec<(Id, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            catalog
                .artist(a.0)
                .map(|x| x.sort_name.as_str())
                .cmp(&catalog.artist(b.0).map(|x| x.sort_name.as_str()))
        })
    });

    let limit = args.usize_value("limit", DEFAULT_LIMIT);
    println!("{}", ui::section("Artists"));
    let mut t = Table::new(&["Artist", "Tracks"]).align(1, Align::Right);
    let total = rows.len();
    for (id, count) in rows.into_iter().take(limit) {
        let Some(artist) = catalog.artist(id) else {
            continue;
        };
        t.push(vec![artist.name.clone(), count.to_string()]);
    }
    print!("{}", t.render());
    announce_limit(total.min(limit), total, "artist");
}
