//! Command implementations.
//!
//! One module per group of related commands. Everything they share — where the
//! catalog lives, how it is loaded, how a role is spelled out — stays here.

mod album;
mod annotate;
mod artist;
mod browse;
mod check;
mod copy;
mod doctor;
mod export;
mod facet;
mod import;
mod inspect;
mod playlist;
mod reset;
mod scan;
mod search;
mod spectrum;
mod stats;
mod track;

pub use album::show_album;
use annotate::panel_for;
pub use annotate::{
    collection, collections, favourites, history, love, note, notes, played, query, rate, tag,
};
pub use artist::show_artist;
pub use browse::{list_albums, list_artists, list_genres, list_labels, list_years};
pub use check::check;
pub use copy::copy;
pub use doctor::show_doctor;
pub use export::export;
pub use facet::{show_genre, show_label};
pub use import::import;
pub use inspect::inspect;
pub use playlist::playlist;
pub use reset::reset;
pub use scan::{roots, scan};
pub use search::search;
pub use spectrum::spectrum;
pub use stats::show_stats;
pub use track::show_track;

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use aede_core::model::{Catalog, Id};
use aede_core::store;
use aede_core::tags::AudioProperties;
use aede_core::text;

use crate::args::{Args, Window};
use crate::ui::{self, Table};

/// What every command returns: nothing useful, or an error already worded for
/// the user.
pub type Res = Result<(), Box<dyn Error>>;

/// Asks before something irreversible, unless `--yes` was given.
///
/// With no terminal to ask on — a script, a pipe — it refuses rather than
/// assuming an answer. Assuming "no" would make a scripted run fail silently;
/// assuming "yes" would destroy something nobody agreed to lose.
///
/// `what` completes both the question and the refusal, so a message names the
/// act rather than talking about "the operation". Shared rather than copied:
/// it lived in `reset` alone until `history --remove` needed the same
/// question, and a second confirmation prompt worded slightly differently is
/// how a user learns to stop reading them.
pub fn confirmed(args: &Args, what: &str) -> Result<bool, Box<dyn Error>> {
    use std::io::{IsTerminal, Write};
    if args.has("yes") {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(format!("no terminal to confirm on: add --yes to {what}").into());
    }
    print!("  Type \"yes\" to confirm: ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(answer.trim().eq_ignore_ascii_case("yes"))
}

/// A folder the user named, resolved the way the catalog stores folders.
///
/// **Every path that arrives from the command line and will be compared against
/// a stored one goes through here.** The catalog keeps its watched roots
/// canonical, and the comparisons that matter — is this file under that root,
/// is this destination inside my library — are string comparisons on a
/// separator boundary ([`aede_core::text::is_under`]). A path reached through a
/// symbolic link names the same folder by a string that never compares equal,
/// so the answer comes back "no" for a folder that plainly is.
///
/// On macOS this is not a corner case but the ordinary one: `/var` is a link to
/// `/private/var` and `/tmp` to `/private/tmp`, so most paths exist in two
/// spellings and only one of them is ever the catalog's.
///
/// The step was written out four times, slightly differently, in `scan`,
/// `roots`, `check` and `copy` — and `copy`, the one command that *writes*, was
/// the one that had left it out. Its "this destination is inside your library"
/// refusal therefore waved through precisely the case it exists to catch. One
/// helper, so that the fifth command cannot forget it.
///
/// A path the filesystem will not resolve comes back as it was: acting on the
/// name given is better than refusing outright.
pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Folders or files a run is restricted to, canonicalized. Empty means the
/// whole catalog.
///
/// Shared by every command that takes `[folder…]` and acts on the files under
/// it — `check`, `spectrum`, `playlist`. They must agree on what "under this
/// folder" means, and on macOS that agreement is not free: `/var` and
/// `/private/var` name the same place by two strings that never compare equal,
/// which is what [`canonical`] exists for.
pub fn scope_of(args: &Args) -> Result<Vec<String>, Box<dyn Error>> {
    let mut scope = Vec::new();
    for raw in &args.positionals {
        let path = Path::new(raw);
        if !path.exists() {
            return Err(format!("\"{raw}\" does not exist").into());
        }
        scope.push(canonical(path).to_string_lossy().to_string());
    }
    Ok(scope)
}

/// `true` when the path is inside one of the folders given, or is one of them.
pub fn in_scope(path: &str, scope: &[String]) -> bool {
    scope.is_empty() || scope.iter().any(|root| text::is_under(path, root))
}

/// Where the catalog lives: what `--data` names, or the default location.
pub fn data_dir(args: &Args) -> PathBuf {
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

/// What the user wrote, reattached to this catalog.
///
/// Shared so that a listing evaluating a query has the ratings and tags a
/// query may ask about, without every command learning where the file is.
fn user_data(args: &Args, catalog: &Catalog) -> Result<aede_core::user::UserData, Box<dyn Error>> {
    let path = aede_core::user::user_path(&data_dir(args));
    let mut data = aede_core::user::load(&path)?.unwrap_or_default();
    aede_core::user::reconcile(&mut data, catalog);
    Ok(data)
}

/// The role vocabulary, as `(key stored, name shown)`.
///
/// One table read in both directions. It used to be a one-way `match`, and the
/// consequence was a message contradicting itself in a single breath: asked for
/// `--role "album artist"` — the only spelling ever shown on screen — the
/// program answered that the artist was *not* credited as album artist and then
/// listed "album artist (14)" among their credits. The user can only type what
/// they are shown; whatever is shown must therefore be accepted.
const ROLE_NAMES: &[(&str, &str)] = &[
    ("main", "main artist"),
    ("album", "album artist"),
    ("composer", "composer"),
    ("conductor", "conductor"),
    ("remixer", "remixer"),
    ("lyricist", "lyricist"),
    ("performer", "performer"),
    ("producer", "producer"),
    ("engineer", "engineer"),
    ("featured", "featured"),
];

/// Display name for an internal role key; unknown keys are shown as they are.
fn role_label(role: &str) -> String {
    ROLE_NAMES
        .iter()
        .find(|(key, _)| *key == role)
        .map(|(_, label)| (*label).to_string())
        .unwrap_or_else(|| role.to_string())
}

/// The key behind whatever the user typed, or `None` if it names no role.
///
/// Accepts the stored key and the displayed name alike, up to case and
/// accents. It looks first at the roles the catalog **holds** — so a role
/// arriving from MusicBrainz at M1, absent from the table above, is reachable
/// by its own name without a line of code — then at the known vocabulary, so
/// that a real role nobody holds here can be told apart from a word that is no
/// role at all. The two deserve different answers: one is an empty result, the
/// other a misunderstanding.
fn role_key(catalog: &Catalog, typed: &str) -> Option<String> {
    let wanted = text::normalize(typed);
    let in_use = catalog
        .roles_in_use()
        .into_iter()
        .find(|key| text::normalize(key) == wanted || text::normalize(&role_label(key)) == wanted)
        .map(|key| key.to_string());
    in_use.or_else(|| {
        ROLE_NAMES
            .iter()
            .find(|(key, label)| text::normalize(key) == wanted || text::normalize(label) == wanted)
            .map(|(key, _)| (*key).to_string())
    })
}

/// The roles a catalog holds, spelled the way they are shown.
///
/// What an error message offers has to be typeable back in: listing the stored
/// keys would name `album` where every screen says `album artist`.
fn roles_offered(catalog: &Catalog) -> String {
    catalog
        .roles_in_use()
        .into_iter()
        .map(role_label)
        .collect::<Vec<_>>()
        .join(", ")
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

/// Says that a listing was cut, and what to do about it.
///
/// Every listing stops at `--limit` rows so as not to fill the terminal, and
/// all five of them used to stop **in silence**. A cut list reads as "that is
/// all there is", which is the one thing it is not: an album sitting past the
/// fiftieth row simply did not exist as far as the user could tell. The header
/// counting the matches is not enough — nobody compares it against the rows
/// they were given.
///
/// Nothing is printed when everything was shown, so the notice keeps meaning
/// something.
fn announce_window(window: Window, total: usize, what: &str) {
    let Some((first, last)) = window.shown(total) else {
        // Two different emptinesses, and naming the wrong one sends the reader
        // looking for a page that was never there. `--offset` explains an empty
        // screen only when there was something to page through; a listing that
        // matched nothing at all is not a paging accident. The confusion became
        // easy to meet the day the listings learned `--query`: `aede artists
        // --query "year:2050"` answered "0 artist in all, and --offset=0 starts
        // past the end", which blames a page number nobody typed.
        let reason = match total {
            0 => format!("nothing here: no {what} to show"),
            _ => format!(
                "nothing here: {} in all, and --offset={} starts past the end",
                ui::plural(total, what),
                window.offset
            ),
        };
        println!("  {}", ui::yellow(&reason));
        return;
    };
    if first == 1 && last == total {
        return;
    }
    println!(
        "  {}",
        ui::yellow(&format!(
            "{first}–{last} of {} — --offset={last} for the next page, --all for every row",
            ui::plural(total, what)
        ))
    );
}

/// Marker put after an album title when the same album sits elsewhere too.
///
/// Two words rather than a symbol: a legend nobody reads is worse than four
/// characters of prose, and the two cases call for opposite reactions.
fn copy_marker(catalog: &Catalog, release_id: Id) -> String {
    use aede_core::model::{DUPLICATE, OTHER_EDITION};
    if !catalog.related_releases(release_id, DUPLICATE).is_empty() {
        return " (duplicate)".to_string();
    }
    if !catalog
        .related_releases(release_id, OTHER_EDITION)
        .is_empty()
    {
        return " (other edition)".to_string();
    }
    String::new()
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
    if args.has("csv") || args.has("json") {
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
