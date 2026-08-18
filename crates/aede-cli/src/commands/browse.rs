//! Flat listings: artists, albums, genres, labels, years.

use aede_core::model::Id;
use aede_core::stats;
use aede_core::text;

use super::{Res, load, totals};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn list_artists(args: &Args) -> Res {
    let catalog = load(args)?;
    let limit = args.usize_value("limit", 50);
    let by_tracks = args.value("sort").unwrap_or("tracks") == "tracks";

    let mut rows: Vec<(Id, usize, usize, u64, u64)> = catalog
        .artists
        .iter()
        .map(|a| {
            let tracks = catalog.tracks_of_artist(a.id);
            let albums = catalog.releases_of_artist(a.id).len();
            let (duration, size) = totals(&catalog, &tracks);
            (a.id, tracks.len(), albums, duration, size)
        })
        .collect();

    if by_tracks {
        rows.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| {
                catalog.artists[a.0 as usize]
                    .sort_name
                    .cmp(&catalog.artists[b.0 as usize].sort_name)
            })
        });
    } else {
        rows.sort_by(|a, b| {
            catalog.artists[a.0 as usize]
                .sort_name
                .cmp(&catalog.artists[b.0 as usize].sort_name)
        });
    }

    println!(
        "{}",
        ui::section(&format!("Artists ({} in total)", catalog.artists.len()))
    );
    let mut t = Table::new(&["Artist", "Tracks", "Albums", "Duration", "Size"])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 50);
    for (id, tracks, albums, duration, size) in rows.into_iter().take(limit) {
        let a = &catalog.artists[id as usize];
        t.push(vec![
            a.name.clone(),
            tracks.to_string(),
            albums.to_string(),
            text::format_duration(duration),
            text::format_size(size),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

pub fn list_albums(args: &Args) -> Res {
    let catalog = load(args)?;
    let limit = args.usize_value("limit", 50);

    let artist_filter = args.value("artist").map(text::normalize);
    let year_filter: Option<u32> = args.value("year").and_then(|v| v.parse().ok());

    let mut rows: Vec<&aede_core::model::Release> = catalog
        .releases
        .iter()
        .filter(|r| match &artist_filter {
            Some(key) => r
                .album_artist_id
                .and_then(|id| catalog.artist(id))
                .map(|a| a.key.contains(key.as_str()))
                .unwrap_or(false),
            None => true,
        })
        .filter(|r| match year_filter {
            Some(year) => r.year == Some(year),
            None => true,
        })
        .collect();

    rows.sort_by(|a, b| {
        a.year
            .unwrap_or(u32::MAX)
            .cmp(&b.year.unwrap_or(u32::MAX))
            .then_with(|| a.title.cmp(&b.title))
    });

    println!(
        "{}",
        ui::section(&format!("Albums ({} matching)", rows.len()))
    );
    let mut t = Table::new(&[
        "Year", "Album", "Artist", "Tracks", "Duration", "Size", "Format",
    ])
    .align(3, Align::Right)
    .align(4, Align::Right)
    .align(5, Align::Right)
    .limit(1, 40)
    .limit(2, 30)
    .limit(6, 30);
    for release in rows.into_iter().take(limit) {
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        let formats: std::collections::BTreeSet<String> = release
            .track_ids
            .iter()
            .filter_map(|&id| catalog.track(id))
            .filter_map(|t| catalog.file(t.file_id))
            .map(|f| f.properties.quality_label())
            .collect();
        let (duration, size) = totals(&catalog, &release.track_ids);
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
            formats.into_iter().collect::<Vec<_>>().join(", "),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

pub fn list_genres(args: &Args) -> Res {
    let catalog = load(args)?;
    let limit = args.usize_value("limit", 50);
    println!(
        "{}",
        ui::section(&format!("Genres ({} in total)", catalog.genres.len()))
    );
    let top = stats::top_genres(&catalog, limit);
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    let mut t = Table::new(&["Genre", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .limit(0, 40);
    for (id, count) in top {
        let name = catalog
            .genre(id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        let (duration, size) = totals(&catalog, &tracks_of_genre(&catalog, id));
        t.push(vec![
            name,
            count.to_string(),
            text::format_duration(duration),
            text::format_size(size),
            ui::bar(count, max, 20),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

pub fn list_labels(args: &Args) -> Res {
    let catalog = load(args)?;
    let limit = args.usize_value("limit", 50);
    println!(
        "{}",
        ui::section(&format!("Labels ({} in total)", catalog.labels.len()))
    );
    let top = stats::top_labels(&catalog, limit);
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    let mut t = Table::new(&["Label", "Albums", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 40);
    for (id, count) in top {
        let name = catalog
            .label(id)
            .map(|l| l.name.clone())
            .unwrap_or_default();
        let tracks = tracks_of_label(&catalog, id);
        let (duration, size) = totals(&catalog, &tracks);
        t.push(vec![
            name,
            count.to_string(),
            tracks.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            ui::bar(count, max, 20),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

pub fn list_years(args: &Args) -> Res {
    let catalog = load(args)?;
    let mut by_year: std::collections::BTreeMap<u32, (usize, Vec<Id>)> = Default::default();
    for release in &catalog.releases {
        let Some(year) = release.year else { continue };
        let entry = by_year.entry(year).or_default();
        entry.0 += 1;
        entry.1.extend(release.track_ids.iter().copied());
    }
    println!("{}", ui::section("Years"));
    let max = by_year.values().map(|(a, _)| *a).max().unwrap_or(0);
    let mut t = Table::new(&["Year", "Albums", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right);
    for (year, (albums, tracks)) in by_year {
        let (duration, size) = totals(&catalog, &tracks);
        t.push(vec![
            year.to_string(),
            albums.to_string(),
            tracks.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            ui::bar(albums, max, 20),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

/// Tracks carrying a genre, directly or through their release.
fn tracks_of_genre(catalog: &aede_core::model::Catalog, genre_id: Id) -> Vec<Id> {
    use aede_core::model::EntityKind;
    let mut tracks: std::collections::BTreeSet<Id> = Default::default();
    for link in &catalog.genre_links {
        if link.genre_id != genre_id {
            continue;
        }
        match link.entity_kind {
            EntityKind::Track => {
                tracks.insert(link.entity_id);
            }
            EntityKind::Release => {
                if let Some(release) = catalog.release(link.entity_id) {
                    tracks.extend(release.track_ids.iter().copied());
                }
            }
            _ => {}
        }
    }
    tracks.into_iter().collect()
}

/// Tracks issued on a label, through the releases carrying it.
fn tracks_of_label(catalog: &aede_core::model::Catalog, label_id: Id) -> Vec<Id> {
    catalog
        .releases
        .iter()
        .filter(|r| r.label_ids.contains(&label_id))
        .flat_map(|r| r.track_ids.iter().copied())
        .collect()
}
