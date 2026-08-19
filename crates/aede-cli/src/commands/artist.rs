//! The `artist` command: one page per artist.
//!
//! Releases are split by role. Performing on somebody else's album is an
//! appearance, not part of a discography, and a writing credit is neither.

use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::text;

use super::{Res, load, selection_output, totals};
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
    let written = catalog.writing_credits_of_artist(artist_id);
    let tracks = catalog.performed_tracks_of_artist(artist_id);
    // Two lines, always labelled. An unlabelled count reads as "everything
    // about this artist", and a band's lyricist — credited on forty albums,
    // audible on none — was announced as "0 album · 0 track" right above the
    // forty rows that contradict it.
    let written_tracks = catalog.written_tracks_of_artist(artist_id);
    print_measures(&catalog, "performing", own.len(), &tracks);
    print_measures(&catalog, "writing", written.len(), &written_tracks);

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
        "Credited as writer or producer",
        &written,
        TrackColumn::WholeRelease,
    );

    let neighbours = catalog.neighbours_of_artist(artist.id);
    if !neighbours.is_empty() {
        println!("{}", ui::section("Played with"));
        let mut t = Table::new(&["Artist", "Tracks in common"])
            .align(1, Align::Right)
            .limit(0, 40);
        for (other, weight, _) in neighbours.iter().take(args.usize_value("limit", 20)) {
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

    let roles = collect_roles(&catalog, artist.id);
    if roles.len() > 1 {
        println!("{}", ui::section("Roles"));
        let mut t = Table::new(&["Role", "Occurrences"]).align(1, Align::Right);
        for (role, count) in roles {
            t.push(vec![role_label(&role), count.to_string()]);
        }
        print!("{}", t.render());
    }
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
            release.title.clone(),
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

/// Display name for an internal role key; unknown keys are shown as they are.
fn role_label(role: &str) -> String {
    match role {
        "main" => "main artist",
        "album" => "album artist",
        "composer" => "composer",
        "conductor" => "conductor",
        "remixer" => "remixer",
        "lyricist" => "lyricist",
        "performer" => "performer",
        "producer" => "producer",
        "engineer" => "engineer",
        "featured" => "featured",
        other => other,
    }
    .to_string()
}

/// Lists the tracks two artists both perform on.
///
/// The pair is what the collaboration weight counts, so the number of rows
/// here always matches the figure shown in the "Played with" table.
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

    println!(
        "{}",
        ui::section(&format!("{here} and {} on the same track", other.name))
    );
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
