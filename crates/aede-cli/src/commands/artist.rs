//! The `artist` command: one page per artist.
//!
//! Releases are split by role. Performing on somebody else's album is an
//! appearance, not part of a discography, and a writing credit is neither.

use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::text;

use super::{
    Res, copy_marker, load, role_key, role_label, roles_offered, selection_output, totals,
};
use crate::args::Args;
use crate::ui::{self, Align, Table};

pub fn show_artist(args: &Args) -> Res {
    let catalog = load(args)?;
    let name = args.positionals.join(" ");
    if name.trim().is_empty() {
        return Err("give a name: aede artist \"Miles Davis\"".into());
    }
    let Some(artist) = catalog.find_artist(&name).or_else(|| {
        // Fall back on the fuzzy search.
        catalog
            .search(&name, 1)
            .first()
            .filter(|h| h.kind == EntityKind::Artist)
            .and_then(|h| catalog.artist(h.id))
    }) else {
        return Err(format!("no artist matches \"{name}\"").into());
    };

    // `--with` turns one line of the collaboration table into the tracks it
    // counts: the graph is only useful if one can walk down it.
    if let Some(wanted) = args.value("with") {
        return print_tracks_in_common(&catalog, artist.id, wanted);
    }

    // `--role` narrows the page to what this person did *in that role*. The
    // page below already separates performing from writing; this goes one step
    // finer, and is the only way to ask "what did Ozzy sing on" as opposed to
    // "what is Ozzy on at all".
    if let Some(role) = args.value("role") {
        return print_tracks_in_role(&catalog, artist.id, role, args);
    }

    // The tracks the artist is audible on, which is what one wants to hear or
    // to tabulate.
    if let Some(result) = selection_output(
        &catalog,
        &catalog.performed_tracks_of_artist(artist.id),
        args,
    ) {
        return result;
    }

    println!("{}", ui::section(&artist.name));
    if artist.sort_name != artist.name {
        println!("  {}", ui::dim(&format!("sort: {}", artist.sort_name)));
    }
    if let Some(mbid) = &artist.mbid {
        println!("  {}", ui::dim(&format!("MusicBrainz: {mbid}")));
    }

    let artist_id = artist.id;
    let own = catalog.releases_as_album_artist(artist_id);
    let guest = catalog.guest_appearances(artist_id);
    let tracks = catalog.performed_tracks_of_artist(artist_id);
    // Two lines, always labelled. An unlabelled count reads as "everything
    // about this artist", and a band's lyricist — credited on forty albums,
    // audible on none — was announced as "0 album · 0 track" right above the
    // forty rows that contradict it.
    //
    // And each line counts **everything** in its class. The writing line used
    // to report the size of the table further down, which leaves out whatever
    // the artist also plays on: Ozzy Osbourne, sixty-nine composer credits and
    // sixty-eight as lyricist, was announced as writing one track. A figure
    // that answers a narrower question than its label is worse than no figure.
    let writing_releases = catalog.releases_with_writing_credit(artist_id);
    let writing_tracks = catalog.writing_tracks_of_artist(artist_id);
    print_measures(&catalog, "performing", own.len(), &tracks);
    print_measures(&catalog, "writing", writing_releases.len(), &writing_tracks);

    // The table below shows only what the two tables above do not, so it needs
    // its own set — and, since the two numbers differ, its own heading.
    let written_elsewhere = catalog.releases_written_without_performing(artist_id);

    let genres = collect_genres_for_artist(&catalog, artist.id);
    if !genres.is_empty() {
        println!("  {} {}", ui::dim("genres:"), genres.join(", "));
    }

    print_release_table(&catalog, "Discography", &own, TrackColumn::WholeRelease);
    print_release_table(
        &catalog,
        "Appears on",
        &guest,
        TrackColumn::OnlyArtist {
            header: "Tracks here",
            artist_id,
        },
    );
    print_release_table(
        &catalog,
        "Written or produced, without performing on it",
        &written_elsewhere,
        TrackColumn::WholeRelease,
    );

    let neighbours = catalog.neighbours_of_artist(artist.id);
    if !neighbours.is_empty() {
        println!("{}", ui::section("Played with"));
        let mut t = Table::new(&["Artist", "Tracks in common"])
            .align(1, Align::Right)
            .limit(0, 40);
        for (other, weight, _) in neighbours.iter().take(args.number_or("limit", 20)?) {
            t.push(vec![other.name.clone(), weight.to_string()]);
        }
        print!("{}", t.render());
        println!(
            "{}",
            ui::dim("  (inferred from the credits found in tags; MusicBrainz will enrich these)")
        );
        println!(
            "{}",
            ui::dim("  aede artist \"<name>\" --with=\"<other>\" lists the tracks in common")
        );
    }

    // Shown as soon as the artist does something beyond being the main credit,
    // even if that is the only thing they do: an artist who is *only* a
    // producer used to get no panel at all, which read as "no role recorded".
    let roles = collect_roles(&catalog, artist.id);
    if roles
        .iter()
        .any(|(role, _)| role != "main" && role != "album")
    {
        println!("{}", ui::section("Roles"));
        let mut t = Table::new(&["Role", "Occurrences"]).align(1, Align::Right);
        for (role, count) in roles {
            t.push(vec![role_label(&role), count.to_string()]);
        }
        print!("{}", t.render());
    }
    // A rating given and never shown again is a rating nobody trusts.
    super::panel_for(args, &catalog, EntityKind::Artist, artist.id);
    Ok(())
}

/// Which release tables the artist page can print.
///
/// A guest appearance concerns only part of the release, so the track column
/// must count the artist's own tracks rather than the whole album.
enum TrackColumn {
    WholeRelease,
    OnlyArtist { header: &'static str, artist_id: Id },
}

/// Renders one of the artist page's release tables.
fn print_release_table(catalog: &Catalog, title: &str, ids: &[Id], column: TrackColumn) {
    if ids.is_empty() {
        return;
    }
    println!("{}", ui::section(title));
    let track_header = match column {
        TrackColumn::WholeRelease => "Tracks",
        TrackColumn::OnlyArtist { header, .. } => header,
    };
    let mut t = Table::new(&[
        "Year",
        "Album",
        "Artist",
        track_header,
        "Duration",
        "Size",
        "Format",
    ])
    .align(3, Align::Right)
    .align(4, Align::Right)
    .align(5, Align::Right)
    .limit(1, 40)
    .limit(2, 24)
    .limit(6, 30);

    let mut list: Vec<&aede_core::model::Release> =
        ids.iter().filter_map(|&id| catalog.release(id)).collect();
    list.sort_by_key(|r| (r.year.unwrap_or(u32::MAX), r.title.clone()));

    for release in list {
        // The whole row describes the same set of tracks. Counting one track
        // and timing the entire album made a guest appearance of one song look
        // like forty minutes of music.
        let counted: Vec<Id> = match column {
            TrackColumn::WholeRelease => release.track_ids.clone(),
            TrackColumn::OnlyArtist { artist_id, .. } => release
                .track_ids
                .iter()
                .copied()
                .filter(|&track_id| {
                    catalog
                        .credits_on(EntityKind::Track, track_id)
                        .iter()
                        .any(|(a, role)| {
                            a.id == artist_id && aede_core::model::is_performing_role(role)
                        })
                })
                .collect(),
        };
        let (duration, size) = totals(catalog, &counted);
        let formats: std::collections::BTreeSet<String> = counted
            .iter()
            .filter_map(|&id| catalog.track(id))
            .filter_map(|t| catalog.file(t.file_id))
            .map(|f| f.properties.quality_label())
            .collect();
        let album_artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        t.push(vec![
            release
                .year
                .map(|y| y.to_string())
                .unwrap_or_else(|| "—".into()),
            format!("{}{}", release.title, copy_marker(catalog, release.id)),
            album_artist,
            counted.len().to_string(),
            text::format_duration(duration),
            text::format_size(size),
            formats.into_iter().collect::<Vec<_>>().join(", "),
        ]);
    }
    print!("{}", t.render());
}

fn collect_genres_for_artist(catalog: &Catalog, artist_id: Id) -> Vec<String> {
    let tracks = catalog.tracks_of_artist(artist_id);
    let mut set = std::collections::BTreeSet::new();
    for track_id in tracks {
        for genre in catalog.genres_of(EntityKind::Track, track_id) {
            set.insert(genre.name.clone());
        }
    }
    set.into_iter().collect()
}

fn collect_roles(catalog: &Catalog, artist_id: Id) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for credit in catalog.credits.iter().filter(|c| c.artist_id == artist_id) {
        *counts.entry(credit.role.clone()).or_insert(0) += 1;
    }
    let mut list: Vec<(String, usize)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list
}

/// Lists the tracks two artists both perform on.
///
/// The pair is what the collaboration weight counts, so the number of rows
/// here always matches the figure shown in the "Played with" table.
/// The tracks one artist holds one role on.
///
/// An empty answer names the roles this person *does* hold: told only that
/// nobody is credited that way, the user cannot tell a wrong spelling from a
/// library whose tags never carried the field.
fn print_tracks_in_role(catalog: &Catalog, artist_id: Id, typed: &str, args: &Args) -> Res {
    let name = catalog
        .artist(artist_id)
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let held = catalog.roles_of_artist(artist_id);
    let listed: Vec<String> = held
        .iter()
        .map(|(role, count)| format!("{} ({count})", role_label(role)))
        .collect();

    // What the user typed is matched against both spellings of every role,
    // because "album artist" is the only one they have ever been shown.
    let Some(role) = role_key(catalog, typed) else {
        return Err(match listed.is_empty() {
            true => format!("{name} carries no credit at all"),
            false => format!(
                "no role is called \"{typed}\".\nRoles in use: {}",
                roles_offered(catalog)
            ),
        }
        .into());
    };
    let tracks = catalog.tracks_of_artist_in_role(artist_id, &role);
    if tracks.is_empty() {
        return Err(match listed.is_empty() {
            true => format!("{name} carries no credit at all"),
            false => format!(
                "{name} is not credited as {}.\nCredited as: {}",
                role_label(&role),
                listed.join(", ")
            ),
        }
        .into());
    }

    if let Some(result) = selection_output(catalog, &tracks, args) {
        return result;
    }
    print_track_table(
        catalog,
        &format!("{name} as {}", role_label(&role)),
        &tracks,
    )
}

fn print_tracks_in_common(catalog: &Catalog, artist_id: Id, wanted: &str) -> Res {
    let Some(other) = catalog.find_artist(wanted).or_else(|| {
        catalog
            .search(wanted, 1)
            .first()
            .filter(|h| h.kind == EntityKind::Artist)
            .and_then(|h| catalog.artist(h.id))
    }) else {
        return Err(format!("no artist matches \"{wanted}\"").into());
    };

    let tracks = catalog.tracks_in_common(artist_id, other.id);
    let here = catalog
        .artist(artist_id)
        .map(|a| a.name.clone())
        .unwrap_or_default();
    if tracks.is_empty() {
        return Err(format!(
            "no track has both {here} and {} performing on it",
            other.name
        )
        .into());
    }
    print_track_table(
        catalog,
        &format!("{here} and {} on the same track", other.name),
        &tracks,
    )
}

/// One table of tracks, with the three measures under it.
///
/// Shared by every way of narrowing an artist page down to a track list — the
/// tracks two people share, the tracks one of them holds a role on — because a
/// track list is a track list, and two renderings of it would drift apart.
fn print_track_table(catalog: &Catalog, heading: &str, tracks: &[Id]) -> Res {
    println!("{}", ui::section(heading));
    let mut t = Table::new(&["Year", "Album", "Track", "Duration", "Size", "Format"])
        .align(3, Align::Right)
        .align(4, Align::Right)
        .limit(1, 32)
        .limit(2, 36);
    let mut rows: Vec<(Option<u32>, String, String, u64, u64, String)> = tracks
        .iter()
        .filter_map(|&id| catalog.track(id))
        .map(|track| {
            let release = track.release_id.and_then(|id| catalog.release(id));
            let file = catalog.file(track.file_id);
            (
                release.and_then(|r| r.year),
                release.map(|r| r.title.clone()).unwrap_or_default(),
                track.title.clone(),
                track.duration_ms.unwrap_or(0),
                file.map(|f| f.size).unwrap_or(0),
                file.map(|f| f.properties.quality_label())
                    .unwrap_or_default(),
            )
        })
        .collect();
    rows.sort_by(|a, b| {
        a.0.unwrap_or(u32::MAX)
            .cmp(&b.0.unwrap_or(u32::MAX))
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    let (mut duration, mut size) = (0u64, 0u64);
    for (year, album, title, track_ms, bytes, format) in rows {
        duration += track_ms;
        size += bytes;
        t.push(vec![
            year.map(|y| y.to_string()).unwrap_or_else(|| "—".into()),
            album,
            title,
            text::format_duration(track_ms),
            text::format_size(bytes),
            format,
        ]);
    }
    print!("{}", t.render());
    println!(
        "  {}",
        ui::dim(&format!(
            "{} · {} · {}",
            ui::plural(tracks.len(), "track"),
            ui::long_duration(duration),
            text::format_size(size)
        ))
    );
    Ok(())
}

/// One summary line: how many releases, how many tracks, and what they weigh.
///
/// `label` says which credits the figures cover, because performing on a
/// record and writing for it are two different presences and the same artist
/// can have both.
fn print_measures(catalog: &Catalog, label: &str, releases: usize, tracks: &[Id]) {
    if releases == 0 && tracks.is_empty() {
        return;
    }
    let (duration, size) = totals(catalog, tracks);
    println!(
        "  {:<11} {} · {} · {} · {}",
        format!("{label}:"),
        ui::plural(releases, "album"),
        ui::plural(tracks.len(), "track"),
        ui::long_duration(duration),
        text::format_size(size)
    );
}
