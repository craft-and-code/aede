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

pub use album::show_album;
pub use artist::show_artist;
pub use browse::{list_albums, list_artists, list_genres, list_labels, list_years};
pub use doctor::show_doctor;
pub use inspect::{export, inspect};
pub use scan::{roots, scan};
pub use search::search;
pub use stats::show_stats;

use std::error::Error;
use std::path::PathBuf;

use aede_core::model::Catalog;
use aede_core::store;

use crate::args::Args;

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
