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

use super::{Res, announce_window, load, selection_output, totals};
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

    println!("{}", ui::section(&names.join(", ")));
    announce_match(kind, &name, &names, "genre");
    print_totals(&catalog, &tracks);

    let releases = releases_holding(&catalog, &tracks);
    print_albums(&catalog, &releases, args)?;
    print_artists(&catalog, &tracks, args)
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
    announce_match(kind, &name, &names, "label");
    print_totals(&catalog, &tracks);
    print_albums(&catalog, &releases, args)?;
    print_artists(&catalog, &tracks, args)
}

/// Says what the page ended up covering, when the heading does not say it
/// already.
///
/// `aede label earache` printed *no label is called "earache"* directly above a
/// heading reading **Earache Records**: a denial and its own refutation, one
/// line apart, while `aede albums --label earache` narrowed on the same text
/// without a word. The note was reporting the mechanism — exact lookup failed,
/// substring lookup ran — where the user asked a question that was answered.
///
/// One name needs no gloss: the heading shows the real name, and that any
/// widening happened is visible in the difference. Several do, because the
/// heading then joins them with a comma and reads as a single name.
fn announce_match(kind: TitleMatch, typed: &str, names: &[String], what: &str) {
    if let Some(line) = match_note(kind, typed, names, what) {
        println!("  {}", ui::dim(&line));
    }
}

/// The note itself, so what it says can be asserted rather than eyeballed.
fn match_note(kind: TitleMatch, typed: &str, names: &[String], what: &str) -> Option<String> {
    if kind == TitleMatch::Exact || names.len() < 2 {
        return None;
    }
    Some(format!(
        "\"{typed}\" matches {}; this page covers them all",
        ui::plural(names.len(), what)
    ))
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

fn print_albums(catalog: &Catalog, releases: &[Id], args: &Args) -> Res {
    if releases.is_empty() {
        return Ok(());
    }
    let window = args.window(DEFAULT_LIMIT)?;
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
    for (release, duration, size) in rows.into_iter().skip(window.offset).take(window.limit) {
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
    announce_window(window, total, "album");
    Ok(())
}

/// How many of these tracks each artist is audible on, most first.
///
/// Performing roles only: a genre is something you *hear*, and counting the
/// lyricist of a track among the artists of a style would be a different claim.
///
/// The count is of **tracks**, so a track counts once however many performing
/// roles the artist holds on it. Counting credits instead reported 57 tracks
/// for a band whose three albums on the label hold 29 — credited both as main
/// artist and as performer on each one, the usual shape of a well-tagged file —
/// a figure the albums table directly above visibly contradicted. Any column
/// headed with a unit counts that unit, not the rows that mention it.
fn tracks_per_artist(catalog: &Catalog, tracks: &[Id]) -> Vec<(Id, usize)> {
    let mut counts: BTreeMap<Id, usize> = BTreeMap::new();
    for &track in tracks {
        let mut on_this_track: std::collections::BTreeSet<Id> = Default::default();
        for (artist, role) in catalog.credits_on(aede_core::model::EntityKind::Track, track) {
            if aede_core::model::is_performing_role(role) {
                on_this_track.insert(artist.id);
            }
        }
        for artist in on_this_track {
            *counts.entry(artist).or_insert(0) += 1;
        }
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
    rows
}

/// Who is audible here, ranked by how much of it they carry.
fn print_artists(catalog: &Catalog, tracks: &[Id], args: &Args) -> Res {
    let rows = tracks_per_artist(catalog, tracks);
    if rows.is_empty() {
        return Ok(());
    }

    let window = args.window(DEFAULT_LIMIT)?;
    println!("{}", ui::section("Artists"));
    let mut t = Table::new(&["Artist", "Tracks"]).align(1, Align::Right);
    let total = rows.len();
    for (id, count) in rows.into_iter().skip(window.offset).take(window.limit) {
        let Some(artist) = catalog.artist(id) else {
            continue;
        };
        t.push(vec![artist.name.clone(), count.to_string()]);
    }
    print!("{}", t.render());
    announce_window(window, total, "artist");
    Ok(())
}

#[cfg(test)]
mod tests {
    use aede_core::model::{Artist, Credit, EntityKind};

    use super::*;

    fn artist(id: Id, name: &str) -> Artist {
        Artist {
            id,
            name: name.into(),
            sort_name: name.into(),
            key: name.to_lowercase(),
            mbid: None,
        }
    }

    fn credit(artist_id: Id, track: Id, role: &str) -> Credit {
        Credit {
            artist_id,
            entity_kind: EntityKind::Track,
            entity_id: track,
            role: role.into(),
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_page_that_answers_does_not_open_by_denying() {
        // `label earache` printed "no label is called \"earache\"" directly
        // above a heading reading "Earache Records", while `albums --label
        // earache` narrowed on the same text without a word. The note was
        // reporting the mechanism — exact lookup failed, substring lookup ran —
        // where the question had in fact been answered.
        assert_eq!(
            match_note(
                TitleMatch::Partial,
                "earache",
                &names(&["Earache Records"]),
                "label"
            ),
            None,
            "one name needs no gloss: the heading is the answer"
        );
        assert_eq!(
            match_note(
                TitleMatch::Exact,
                "Columbia",
                &names(&["Columbia"]),
                "label"
            ),
            None
        );

        // Several do, because the heading joins them with a comma and reads as
        // a single name.
        assert_eq!(
            match_note(
                TitleMatch::Partial,
                "metal",
                &names(&["Black Metal", "Doom Metal", "Metal"]),
                "genre"
            )
            .as_deref(),
            Some("\"metal\" matches 3 genres; this page covers them all")
        );
    }

    #[test]
    fn an_artist_counts_once_per_track_however_many_roles_they_hold() {
        // A well-tagged file carries ARTIST and PERFORMER, so the band is
        // credited twice on every track of its own album. The label page read
        // that as 57 tracks for a band whose albums on the page held 29, right
        // under an albums table that added up to 29.
        let mut catalog = Catalog {
            artists: vec![artist(0, "Deicide"), artist(1, "Steve Asheim")],
            ..Default::default()
        };
        for track in 0..3 {
            catalog.credits.push(credit(0, track, "main"));
            catalog.credits.push(credit(0, track, "performer"));
            // A non-performing role stays out of the reckoning entirely.
            catalog.credits.push(credit(1, track, "lyricist"));
        }
        // And one track where the drummer is heard as well as credited.
        catalog.credits.push(credit(1, 0, "performer"));

        let rows = tracks_per_artist(&catalog, &[0, 1, 2]);
        assert_eq!(rows, vec![(0, 3), (1, 1)], "counted rows, not tracks");
    }

    #[test]
    fn no_artist_can_carry_more_tracks_than_the_page_holds() {
        // The bound the printed page must always satisfy, whatever the tags do.
        let mut catalog = Catalog {
            artists: vec![artist(0, "Bolt Thrower")],
            ..Default::default()
        };
        for track in 0..9 {
            for role in ["main", "performer", "conductor", "remixer"] {
                catalog.credits.push(credit(0, track, role));
            }
        }
        let tracks: Vec<Id> = (0..9).collect();
        for (_, count) in tracks_per_artist(&catalog, &tracks) {
            assert!(count <= tracks.len(), "{count} of {}", tracks.len());
        }
    }
}
