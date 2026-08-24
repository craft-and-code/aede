//! Flat listings: artists, albums, genres, labels, years.

use aede_core::model::Id;
use aede_core::stats;
use aede_core::text;

use super::{
    Res, announce_window, copy_marker, export, load, role_key, role_label, roles_offered, totals,
};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn list_artists(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;
    // Read strictly, like the window: `--sort banana` used to fall through to
    // sorting by name, which is an answer to a question nobody asked and looks
    // exactly like a correct one.
    let by_tracks = match args.value("sort") {
        None | Some("tracks") => true,
        Some("name") => false,
        Some(other) => {
            return Err(format!("--sort takes \"tracks\" or \"name\": got \"{other}\"").into());
        }
    };

    // Who does *this* in my library — the inverse of the artist page, and the
    // whole reason credits carry a role rather than being a bare artist
    // column. A role nobody is credited with is an error, with the list of the
    // ones that exist: guessing the spelling of a role is not the user's job.
    let role: Option<String> = match args.value("role") {
        Some(typed) => match role_key(&catalog, typed) {
            Some(key) => Some(key),
            None => {
                return Err(format!(
                    "no role is called \"{typed}\".\nRoles in use: {}",
                    roles_offered(&catalog)
                )
                .into());
            }
        },
        None => None,
    };
    let in_role: Option<std::collections::BTreeSet<Id>> = match role.as_ref() {
        Some(role) => {
            let found: std::collections::BTreeSet<Id> = catalog
                .artists_in_role(role)
                .into_iter()
                .map(|(id, _)| id)
                .collect();
            // A real role that this library happens not to hold: an empty
            // result, not a misunderstanding, and the two read differently.
            if found.is_empty() {
                return Err(format!(
                    "nobody here is credited as {}.\nRoles in use: {}",
                    role_label(role),
                    roles_offered(&catalog)
                )
                .into());
            }
            Some(found)
        }
        None => None,
    };

    let mut rows: Vec<(Id, usize, usize, u64, u64)> = catalog
        .artists
        .iter()
        .filter(|a| match &in_role {
            Some(ids) => ids.contains(&a.id),
            None => true,
        })
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

    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = rows
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .map(|&(id, tracks, albums, duration, size)| {
                let a = &catalog.artists[id as usize];
                vec![
                    a.name.clone(),
                    a.sort_name.clone(),
                    tracks.to_string(),
                    albums.to_string(),
                    duration.to_string(),
                    size.to_string(),
                    a.mbid.clone().unwrap_or_default(),
                ]
            })
            .collect();
        return export::rows_table(
            &[
                "artist",
                "sort_name",
                "tracks",
                "albums",
                "duration_ms",
                "size_bytes",
                "musicbrainz_artistid",
            ],
            &table,
            args,
        );
    }

    let heading = match &role {
        Some(role) => format!("Artists credited as {} ({})", role_label(role), rows.len()),
        None => format!("Artists ({} in total)", catalog.artists.len()),
    };
    println!("{}", ui::section(&heading));
    let mut t = Table::new(&["Artist", "Tracks", "Albums", "Duration", "Size"])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 50);
    let total = rows.len();
    for (id, tracks, albums, duration, size) in
        rows.into_iter().skip(window.offset).take(window.limit)
    {
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
    announce_window(window, total, "artist");
    Ok(())
}

pub fn list_albums(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;

    let artist_filter = args.value("artist").map(text::normalize);
    // A year that does not parse used to become "no filter at all": the whole
    // library came back under a name that asked for one year, and nothing said
    // the filter had been dropped. Same fault as an option silently ignored.
    let year_filter: Option<u32> = match args.value("year") {
        None => None,
        Some(raw) => Some(
            raw.parse()
                .map_err(|_| format!("--year expects a year: --year=1969, not \"{raw}\""))?,
        ),
    };

    // A compilation is a release with no album artist: several artists share
    // it, which is exactly why nothing else in the program can single them out.
    // The two flags are opposites and cannot both be honoured.
    if args.has("compilations") && args.has("no-compilations") {
        return Err("--compilations and --no-compilations ask for opposite things".into());
    }
    let compilations_only = args.has("compilations");
    let albums_only = args.has("no-compilations");

    // `--genre` and `--label` were declared among the options and honoured
    // nowhere: accepted everywhere, ignored everywhere. A filter that does
    // nothing answers about the whole library under a name that promised a
    // part of it.
    let genre_ids = matching_ids(args.value("genre"), |key| {
        catalog
            .genres
            .iter()
            .filter(|g| g.key.contains(key))
            .map(|g| g.id)
            .collect()
    });
    let label_ids = matching_ids(args.value("label"), |key| {
        catalog
            .labels
            .iter()
            .filter(|l| l.key.contains(key))
            .map(|l| l.id)
            .collect()
    });
    if let Some(ids) = &genre_ids
        && ids.is_empty()
    {
        return Err(format!(
            "no genre matches \"{}\".\nRun \"aede genres\" for the list.",
            args.value("genre").unwrap_or_default()
        )
        .into());
    }
    if let Some(ids) = &label_ids
        && ids.is_empty()
    {
        return Err(format!(
            "no label matches \"{}\".\nRun \"aede labels\" for the list.",
            args.value("label").unwrap_or_default()
        )
        .into());
    }
    let of_genre: Option<std::collections::BTreeSet<Id>> = genre_ids.map(|ids| {
        ids.iter()
            .flat_map(|&id| catalog.releases_of_genre(id))
            .collect()
    });
    let with_comment: Option<std::collections::BTreeSet<Id>> = args.value("comment").map(|text| {
        catalog
            .tracks_with_comment(text)
            .into_iter()
            .filter_map(|t| catalog.track(t).and_then(|t| t.release_id))
            .collect()
    });

    let mut rows: Vec<&aede_core::model::Release> = catalog
        .releases
        .iter()
        .filter(|r| {
            if compilations_only {
                return r.is_compilation;
            }
            if albums_only {
                return !r.is_compilation;
            }
            true
        })
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
        .filter(|r| match &of_genre {
            Some(ids) => ids.contains(&r.id),
            None => true,
        })
        .filter(|r| match &label_ids {
            Some(ids) => ids.iter().any(|id| r.label_ids.contains(id)),
            None => true,
        })
        .filter(|r| match &with_comment {
            Some(ids) => ids.contains(&r.id),
            None => true,
        })
        .collect();

    rows.sort_by(|a, b| {
        a.year
            .unwrap_or(u32::MAX)
            .cmp(&b.year.unwrap_or(u32::MAX))
            .then_with(|| a.title.cmp(&b.title))
    });

    if args.has("csv") || args.has("json") {
        // The same table as `export --csv`, restricted to what the filters kept:
        // one file for a whole discography is the usual reason to ask.
        let ids: Vec<Id> = rows
            .iter()
            .skip(window.offset)
            .take(window.limit)
            .map(|r| r.id)
            .collect();
        return export::albums_table(&catalog, &ids, args);
    }

    let heading = if compilations_only {
        "Compilations"
    } else if albums_only {
        "Albums, compilations left out"
    } else {
        "Albums"
    };
    println!(
        "{}",
        ui::section(&format!("{heading} ({} matching)", rows.len()))
    );
    // What was narrowed, spelled out. A filter that leaves the count unchanged
    // is indistinguishable from a filter that was ignored, and this project has
    // shipped two options that really were ignored.
    let mut applied: Vec<String> = Vec::new();
    for (name, value) in [
        ("artist", args.value("artist")),
        ("year", args.value("year")),
        ("genre", args.value("genre")),
        ("label", args.value("label")),
        ("comment", args.value("comment")),
    ] {
        if let Some(value) = value {
            applied.push(format!("{name} \"{value}\""));
        }
    }
    if !applied.is_empty() {
        println!(
            "  {}",
            ui::dim(&format!("filtered on {}", applied.join(", ")))
        );
    }
    let mut t = Table::new(&[
        "Year", "Album", "Artist", "Tracks", "Duration", "Size", "Format",
    ])
    .align(3, Align::Right)
    .align(4, Align::Right)
    .align(5, Align::Right)
    .limit(1, 40)
    .limit(2, 30)
    .limit(6, 30);
    let total = rows.len();
    for release in rows.into_iter().skip(window.offset).take(window.limit) {
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
            format!("{}{}", release.title, copy_marker(&catalog, release.id)),
            artist,
            release.track_ids.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            formats.into_iter().collect::<Vec<_>>().join(", "),
        ]);
    }
    print!("{}", t.render());
    announce_window(window, total, "album");
    Ok(())
}

pub fn list_genres(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;
    // Ranked in full, cut at display: the notice below can only be honest if
    // the count of what was left out is known.
    let top = stats::top_genres(&catalog, usize::MAX);
    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = top
            .iter()
            .map(|&(id, count)| {
                let tracks = tracks_of_genre(&catalog, id);
                let (duration, size) = totals(&catalog, &tracks);
                vec![
                    catalog
                        .genre(id)
                        .map(|g| g.name.clone())
                        .unwrap_or_default(),
                    count.to_string(),
                    duration.to_string(),
                    size.to_string(),
                ]
            })
            .collect();
        return export::rows_table(
            &["genre", "tracks", "duration_ms", "size_bytes"],
            &table,
            args,
        );
    }
    println!(
        "{}",
        ui::section(&format!("Genres ({} in total)", catalog.genres.len()))
    );
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    let mut t = Table::new(&["Genre", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .limit(0, 40);
    let total = top.len();
    for (id, count) in top.into_iter().skip(window.offset).take(window.limit) {
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
    announce_window(window, total, "genre");
    Ok(())
}

pub fn list_labels(args: &Args) -> Res {
    let catalog = load(args)?;
    let window = args.window(50)?;
    let top = stats::top_labels(&catalog, usize::MAX);
    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = top
            .iter()
            .map(|&(id, count)| {
                let tracks = tracks_of_label(&catalog, id);
                let (duration, size) = totals(&catalog, &tracks);
                vec![
                    catalog
                        .label(id)
                        .map(|l| l.name.clone())
                        .unwrap_or_default(),
                    count.to_string(),
                    tracks.len().to_string(),
                    duration.to_string(),
                    size.to_string(),
                ]
            })
            .collect();
        return export::rows_table(
            &["label", "albums", "tracks", "duration_ms", "size_bytes"],
            &table,
            args,
        );
    }
    println!(
        "{}",
        ui::section(&format!("Labels ({} in total)", catalog.labels.len()))
    );
    let max = top.first().map(|(_, n)| *n).unwrap_or(0);
    let mut t = Table::new(&["Label", "Albums", "Tracks", "Duration", "Size", ""])
        .align(1, Align::Right)
        .align(2, Align::Right)
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(0, 40);
    let total = top.len();
    for (id, count) in top.into_iter().skip(window.offset).take(window.limit) {
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
    announce_window(window, total, "label");
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
    if args.has("csv") || args.has("json") {
        let table: Vec<Vec<String>> = by_year
            .iter()
            .map(|(year, (albums, tracks))| {
                let (duration, size) = totals(&catalog, tracks);
                vec![
                    year.to_string(),
                    albums.to_string(),
                    tracks.len().to_string(),
                    duration.to_string(),
                    size.to_string(),
                ]
            })
            .collect();
        return export::rows_table(
            &["year", "albums", "tracks", "duration_ms", "size_bytes"],
            &table,
            args,
        );
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
/// Ids of the entities whose normalized name contains what was asked for.
///
/// `None` when the option was not given at all, which is not the same as an
/// empty result: one means "no filter", the other means "a filter that matches
/// nothing", and the second is an error rather than an empty listing.
fn matching_ids(wanted: Option<&str>, lookup: impl Fn(&str) -> Vec<Id>) -> Option<Vec<Id>> {
    let key = text::normalize(wanted?);
    Some(lookup(&key))
}

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
