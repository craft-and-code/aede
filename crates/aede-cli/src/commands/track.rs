//! The `track` command: one page per track, found by title.
//!
//! Same information as `file`, reached by the name of the music instead of the
//! path of a file. It reads the catalog, never the disk, and therefore knows
//! things a file cannot tell on its own: the release the track belongs to, its
//! position, who is credited on it.
//!
//! A title is not an identifier. Every track carrying it is shown, in full,
//! because the album version, the single and the live rendition are different
//! recordings and the difference is exactly what one wants to see.

use aede_core::json::Json;
use aede_core::model::{Catalog, EntityKind, Id, TitleMatch, Track};
use aede_core::text;

use super::{
    Res, announce_window, load, properties_table, role_label, selection_output, tags_table,
};
use crate::args::Args;
use crate::ui::{self, Table};

/// Default number of pages printed, so that a title as common as "Intro" does
/// not fill the terminal. Raised with `--limit`.
const DEFAULT_LIMIT: usize = 10;

pub fn show_track(args: &Args) -> Res {
    let catalog = load(args)?;
    let title = args.positionals.join(" ");
    if title.trim().is_empty() {
        return Err("give a title: aede track \"Patient Number 9\"".into());
    }

    let (found, kind) = catalog.find_tracks(&title);
    let before_filters = found.len();
    let matches: Vec<&Track> = found
        .into_iter()
        .filter(|t| keeps_artist(&catalog, t, args.value("artist")))
        .filter(|t| keeps_album(&catalog, t, args.value("album")))
        .filter(|t| keeps_comment(&catalog, t, args.value("comment")))
        .collect();

    if matches.is_empty() {
        // Saying "no such title" when the title exists and it is the filter
        // that excluded everything would send the user looking in the wrong
        // place.
        return Err(match before_filters {
            0 => format!("no track matches \"{title}\""),
            n => format!(
                "{} titled \"{title}\", none matching the filters given",
                ui::plural(n, "track")
            ),
        }
        .into());
    }
    let total = matches.len();
    let window = args.window(DEFAULT_LIMIT)?;
    let matches: Vec<&Track> = matches
        .into_iter()
        .skip(window.offset)
        .take(window.limit)
        .collect();

    let ids: Vec<Id> = matches.iter().map(|t| t.id).collect();
    // Its own JSON shape answers first, for the same reason as `search`: this
    // one carries the credits and the technical detail, which no flat table of
    // a selection can.
    if args.has("json") {
        let json = Json::Arr(matches.iter().map(|t| as_json(&catalog, t)).collect());
        println!("{}", json.to_string_pretty());
        return Ok(());
    }
    if let Some(result) = selection_output(&catalog, &ids, args) {
        return result;
    }

    if kind == TitleMatch::Partial {
        println!(
            "  {}",
            ui::dim(&format!(
                "no track is titled \"{title}\"; showing the titles containing it"
            ))
        );
    }
    for track in &matches {
        print_track(&catalog, track);
    }

    // A truncated list must say so: a silent cut reads as "that is all there
    // is", which is the one thing it is not.
    println!();
    if total > matches.len() {
        announce_window(window, total, "track");
        println!(
            "  {}",
            ui::dim("or narrow it down with --artist, --album or --comment")
        );
    } else if total > 1 {
        println!("  {}", ui::dim(&ui::plural(total, "track")));
    }
    Ok(())
}

/// `true` when the track has no artist filter to satisfy, or satisfies it.
///
/// Every credit counts, not just the performers: filtering a title by its
/// composer is as legitimate as filtering it by its singer.
fn keeps_artist(catalog: &Catalog, track: &Track, wanted: Option<&str>) -> bool {
    let Some(wanted) = wanted else {
        return true;
    };
    let key = text::normalize(wanted);
    if key.is_empty() {
        return true;
    }
    let credited = catalog
        .credits_on(EntityKind::Track, track.id)
        .into_iter()
        .any(|(artist, _)| text::normalize(&artist.name).contains(&key));
    let album_artist = track
        .release_id
        .and_then(|id| catalog.release(id))
        .and_then(|r| r.album_artist_id)
        .and_then(|id| catalog.artist(id))
        .is_some_and(|a| text::normalize(&a.name).contains(&key));
    credited || album_artist
}

/// Keeps a track whose file carries a comment containing the text.
///
/// The comment is the one field the user wrote themselves, so it is the one
/// field where their own vocabulary lives: "vinyl rip", "to replace", "from
/// the 2009 remaster". Matching is on the normalized form, since prose is
/// typed carelessly.
fn keeps_comment(catalog: &Catalog, track: &Track, wanted: Option<&str>) -> bool {
    let Some(wanted) = wanted else {
        return true;
    };
    let key = text::normalize(wanted);
    if key.is_empty() {
        return true;
    }
    catalog
        .comment_of_track(track.id)
        .is_some_and(|c| text::normalize(c).contains(&key))
}

/// `true` when the track has no album filter to satisfy, or satisfies it.
fn keeps_album(catalog: &Catalog, track: &Track, wanted: Option<&str>) -> bool {
    let Some(wanted) = wanted else {
        return true;
    };
    let key = text::normalize(wanted);
    if key.is_empty() {
        return true;
    }
    track
        .release_id
        .and_then(|id| catalog.release(id))
        .is_some_and(|r| r.key.contains(&key))
}

fn print_track(catalog: &Catalog, track: &Track) {
    println!("{}", ui::section(&track.title));

    let release = track.release_id.and_then(|id| catalog.release(id));
    let mut context = Table::plain(2);
    if let Some(release) = release {
        let album = match release.year {
            Some(year) => format!("{} ({year})", release.title),
            None => release.title.clone(),
        };
        context.push(vec!["Album".into(), album]);
        let artist = release
            .album_artist_id
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Various Artists".into());
        context.push(vec!["Album artist".into(), artist]);
    }
    if let Some(position) = position(track) {
        context.push(vec!["Position".into(), position]);
    }
    let genres: Vec<String> = catalog
        .genres_of(EntityKind::Track, track.id)
        .into_iter()
        .chain(
            release
                .map(|r| catalog.genres_of(EntityKind::Release, r.id))
                .unwrap_or_default(),
        )
        .map(|g| g.name.clone())
        .collect();
    if !genres.is_empty() {
        context.push(vec!["Genres".into(), dedupe(genres).join(", ")]);
    }
    if let Some(isrc) = &track.isrc {
        context.push(vec!["ISRC".into(), isrc.clone()]);
    }
    let file = catalog.file(track.file_id);
    if let Some(file) = file {
        context.push(vec!["Path".into(), file.path.clone()]);
    }
    context.push(vec!["Integrity".into(), integrity_line(track, catalog)]);
    print!("{}", context.render());

    if let Some(file) = file {
        println!();
        print!(
            "{}",
            properties_table(&file.properties, file.has_embedded_art, file.size).render()
        );
    }

    let credits = catalog.credits_on(EntityKind::Track, track.id);
    if !credits.is_empty() {
        println!("{}", ui::section("Credits"));
        let mut by_role: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for (artist, role) in credits {
            by_role
                .entry(role_label(role))
                .or_default()
                .push(artist.name.clone());
        }
        let mut t = Table::new(&["Role", "Artists"]).limit(1, 60);
        for (role, names) in by_role {
            t.push(vec![role, dedupe(names).join(", ")]);
        }
        print!("{}", t.render());
    }

    if let Some(file) = file {
        print_analyses(catalog, file);
        println!("{}", ui::section("Tags"));
        if file.tags.is_empty() {
            println!("  {}", ui::yellow("no tag in this file"));
        } else {
            print!("{}", tags_table(&file.tags).render());
        }
    }
}

/// Disc and track number, spelled out; `None` when the tags gave neither.
fn position(track: &Track) -> Option<String> {
    match (track.disc_no, track.track_no) {
        (Some(disc), Some(no)) => Some(format!("disc {disc}, track {no}")),
        (None, Some(no)) => Some(format!("track {no}")),
        (Some(disc), None) => Some(format!("disc {disc}")),
        (None, None) => None,
    }
}

fn dedupe(mut names: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    names.retain(|n| seen.insert(n.clone()));
    names
}

fn as_json(catalog: &Catalog, track: &Track) -> Json {
    let mut o = Json::obj();
    o.set("id", track.id.into());
    o.set("title", track.title.clone().into());
    o.set("disc_no", track.disc_no.into());
    o.set("track_no", track.track_no.into());
    o.set("duration_ms", track.duration_ms.into());
    o.set("isrc", track.isrc.clone().into());

    let release = track.release_id.and_then(|id| catalog.release(id));
    o.set(
        "release",
        release.map(|r| r.title.clone()).unwrap_or_default().into(),
    );
    o.set("year", release.and_then(|r| r.year).into());
    o.set(
        "album_artist",
        release
            .and_then(|r| r.album_artist_id)
            .and_then(|id| catalog.artist(id))
            .map(|a| a.name.clone())
            .unwrap_or_default()
            .into(),
    );

    let credits = Json::Arr(
        catalog
            .credits_on(EntityKind::Track, track.id)
            .into_iter()
            .map(|(artist, role)| {
                let mut c = Json::obj();
                c.set("artist", artist.name.clone().into());
                c.set("role", role.to_string().into());
                c
            })
            .collect(),
    );
    o.set("credits", credits);

    if let Some(file) = catalog.file(track.file_id) {
        o.set("path", file.path.clone().into());
        o.set("size", file.size.into());
        o.set("codec", file.properties.codec.clone().into());
        o.set("container", file.properties.container.clone().into());
        o.set("sample_rate", file.properties.sample_rate.into());
        o.set("bit_depth", file.properties.bit_depth.map(u32::from).into());
        o.set("channels", file.properties.channels.map(u32::from).into());
        o.set("bitrate_kbps", file.properties.bitrate_kbps.into());
        o.set("lossless", file.properties.lossless.into());
        let mut tags = Json::obj();
        for (key, values) in &file.tags {
            tags.set(key, values.join(" / ").into());
        }
        o.set("tags", tags);
    }
    o
}

/// What the last integrity check said about the file behind a track.
///
/// "not verified" is a state of its own, and saying so is the point: a blank
/// here would read as "fine".
fn integrity_line(track: &Track, catalog: &Catalog) -> String {
    use aede_core::audit::integrity::Verdict;
    let Some(record) = catalog
        .file(track.file_id)
        .and_then(|f| f.integrity.as_ref())
    else {
        return "not verified — run aede check".to_string();
    };
    match &record.verdict {
        Verdict::Intact => format!("intact ({})", record.method),
        Verdict::NothingToCheck => "the container carries no checksum".to_string(),
        Verdict::Damaged { detail } => format!("damaged — {detail}"),
    }
}

/// Shows what another tool measured on this file, when something was imported.
///
/// Attributed by name, and kept apart from Aède's own panel above: the reader
/// has to be able to tell which program said what, especially when the two
/// disagree.
fn print_analyses(catalog: &Catalog, file: &aede_core::model::AudioFile) {
    for record in catalog.analyses_of(file) {
        let stale = if record.still_applies(file.size, file.mtime) {
            String::new()
        } else {
            " — stale: the file changed since".to_string()
        };
        println!(
            "{}",
            ui::section(&format!("Analysed by {}{stale}", record.source))
        );
        let mut t = Table::plain(2);
        let mut row = |label: &str, value: Option<String>| {
            if let Some(value) = value {
                t.push(vec![label.into(), value]);
            }
        };
        row("MD5", record.md5_state.clone());
        row(
            "Real bit depth",
            record.real_bit_depth.map(|b| format!("{b} bits")),
        );
        row("Fake stereo", record.fake_stereo.map(yes_no));
        row("Transcoding", record.transcoding.clone());
        row("Upscaled", record.upscaling.map(yes_no));
        row("Upsampled", record.upsampling.map(yes_no));
        row(
            "Cutoff",
            record.cutoff_hz.map(|hz| format!("{:.1} kHz", hz / 1000.0)),
        );
        row(
            "Dynamic range",
            record.dr_db.map(|db| format!("{db:.1} dB")),
        );
        row("Peak", record.peak_dbfs.map(|db| format!("{db:.2} dBFS")));
        row(
            "True peak",
            record.true_peak_dbtp.map(|db| format!("{db:.2} dBTP")),
        );
        row(
            "Clipped samples",
            record.clipped_samples.map(|n| n.to_string()),
        );
        row("Verdict", record.detail.clone().or(record.summary.clone()));
        row("Error", record.error.clone());
        print!("{}", t.render());
    }
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}
