//! Command implementations.
//!
//! One module per group of related commands. Everything they share — where the
//! catalog lives, how it is loaded, how a role is spelled out — stays here.

mod album;
mod artist;
mod browse;
mod doctor;
mod inspect;
mod scan;
mod search;
mod stats;
mod track;

pub use album::show_album;
pub use artist::show_artist;
pub use browse::{list_albums, list_artists, list_genres, list_labels, list_years};
pub use doctor::show_doctor;
pub use inspect::{export, inspect};
pub use scan::{roots, scan};
pub use search::search;
pub use stats::show_stats;
pub use track::show_track;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use aede_core::model::Catalog;
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
