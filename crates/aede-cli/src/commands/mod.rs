//! Command implementations.
//!
//! One module per group of related commands. Everything they share — where the
//! catalog lives, how it is loaded, how a role is spelled out — stays here.

mod album;
mod artist;
mod browse;
mod check;
mod doctor;
mod export;
mod inspect;
mod scan;
mod search;
mod stats;
mod track;

pub use album::show_album;
pub use artist::show_artist;
pub use browse::{list_albums, list_artists, list_genres, list_labels, list_years};
pub use check::check;
pub use doctor::show_doctor;
pub use export::export;
pub use inspect::inspect;
pub use scan::{roots, scan};
pub use search::search;
pub use stats::show_stats;
pub use track::show_track;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use aede_core::model::{Catalog, Id};
use aede_core::store;
use aede_core::tags::AudioProperties;
use aede_core::text;

use crate::args::Args;
use crate::ui::Table;

/// What every command returns: nothing useful, or an error already worded for
/// the user.
pub type Res = Result<(), Box<dyn Error>>;

fn data_dir(args: &Args) -> PathBuf {
    args.value("data")
        .map(PathBuf::from)
        .unwrap_or_else(store::default_data_dir)
}

fn load(args: &Args) -> Result<Catalog, Box<dyn Error>> {
    let path = store::catalog_path(&data_dir(args));
    match store::load(&path)? {
        Some(catalog) => Ok(catalog),
        None => Err(format!(
            "no catalog in {}.\nRun this first: aede scan <folder>",
            data_dir(args).display()
        )
        .into()),
    }
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

/// Technical description of a stream, shown identically by `file` and by
/// `track` — one is read from the disk, the other from the catalog, and the
/// reader has no reason to see a difference.
fn properties_table(properties: &AudioProperties, has_embedded_art: bool, size: u64) -> Table {
    let mut t = Table::plain(2);
    let dash = || "—".to_string();
    t.push(vec!["Container".into(), properties.container.clone()]);
    t.push(vec!["Codec".into(), properties.codec.clone()]);
    t.push(vec!["Quality".into(), properties.quality_label()]);
    t.push(vec![
        "Sample rate".into(),
        properties
            .sample_rate
            .map(|r| format!("{r} Hz"))
            .unwrap_or_else(dash),
    ]);
    t.push(vec![
        "Bit depth".into(),
        properties
            .bit_depth
            .map(|b| format!("{b} bits"))
            .unwrap_or_else(dash),
    ]);
    t.push(vec![
        "Channels".into(),
        properties
            .channels
            .map(|c| c.to_string())
            .unwrap_or_else(dash),
    ]);
    t.push(vec![
        "Duration".into(),
        properties
            .duration_ms
            .map(text::format_duration)
            .unwrap_or_else(dash),
    ]);
    t.push(vec![
        "Bitrate".into(),
        properties
            .bitrate_kbps
            .map(|b| format!("{b} kbps"))
            .unwrap_or_else(dash),
    ]);
    t.push(vec![
        "Lossless".into(),
        if properties.lossless { "yes" } else { "no" }.into(),
    ]);
    t.push(vec![
        "Embedded cover art".into(),
        if has_embedded_art { "yes" } else { "no" }.into(),
    ]);
    if size > 0 {
        t.push(vec!["Size".into(), text::format_size(size)]);
    }
    t
}

/// The tags as they were read, one row per field.
fn tags_table(fields: &BTreeMap<String, Vec<String>>) -> Table {
    let mut t = Table::new(&["Field", "Value"]).limit(1, 70);
    for (key, values) in fields {
        t.push(vec![key.clone(), values.join(" / ")]);
    }
    t
}

/// Playing time and size on disk of a set of tracks.
///
/// Count, duration and size are the three measures every listing and every
/// entity page carries: what a part of the library weighs should not depend on
/// the command used to look at it.
fn totals(catalog: &Catalog, tracks: &[Id]) -> (u64, u64) {
    tracks
        .iter()
        .filter_map(|&id| catalog.track(id))
        .fold((0, 0), |(duration, size), track| {
            (
                duration + track.duration_ms.unwrap_or(0),
                size + catalog.file(track.file_id).map(|f| f.size).unwrap_or(0),
            )
        })
}

/// Hands the tracks on screen to another program, when asked for.
///
/// `None` means nothing was asked and the command should print its usual page.
/// Every command that shows a track list offers the same two exits, so the
/// option means the same thing wherever it is typed.
fn selection_output(catalog: &Catalog, tracks: &[Id], args: &Args) -> Option<Res> {
    if args.has("m3u") {
        return Some(play_list(catalog, tracks, args));
    }
    if args.has("csv") {
        if tracks.is_empty() {
            return Some(Err("nothing to put in a table".into()));
        }
        return Some(export::tracks_table(catalog, tracks, args));
    }
    None
}

/// Prints the tracks shown as an M3U playlist instead of the usual page.
///
/// Every command that puts a track list on screen can hand it to a player;
/// that the selection was reached through an album, an artist or a search is
/// the caller's business, not the playlist's.
fn play_list(catalog: &Catalog, tracks: &[Id], args: &Args) -> Res {
    if tracks.is_empty() {
        return Err("nothing to put in a playlist".into());
    }
    export::emit(args, &export::m3u(catalog, tracks))
}
