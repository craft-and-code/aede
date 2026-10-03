//! M3U playlists: the one place that knows how to write one.
//!
//! Two callers with two purposes. `--m3u` hands whatever is on screen to a
//! player, so it writes **absolute** paths to a file that may end up anywhere.
//! `aede playlist` writes into the album folder itself, so it writes
//! **relative** ones: a playlist beside its music travels with it, survives the
//! folder being moved or copied to a card, and is the same file on another
//! machine. One renderer for both, because two would agree about `#EXTINF`
//! today and disagree about it in six months.

use std::path::Path;

use crate::model::{Catalog, EntityKind, Id};

/// Which of the two M3U dialects to write.
#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    /// `#EXTM3U` with an `#EXTINF` line per track: duration and title, which
    /// is what lets a player show a name rather than a file name.
    Extended,
    /// Paths and nothing else. Older hardware players — car head units, some
    /// DAPs — stop at the first `#` they do not understand.
    Simple,
}

/// Renders a playlist.
///
/// `base` is the folder the file will be written into: paths under it are
/// written relative to it, anything else absolute. A playlist that silently
/// dropped the tracks it could not make relative would be worse than one that
/// names them the long way.
///
/// Paths containing CR or LF cannot occupy one M3U line and are omitted. Use
/// [`try_render`] to refuse such a selection instead of producing a partial list.
pub fn render(catalog: &Catalog, tracks: &[Id], base: Option<&Path>, style: Style) -> String {
    render_with_paths(catalog, tracks, style, |id| {
        catalog
            .track(id)
            .and_then(|track| catalog.file(track.file_id))
            .map(|file| relative_to(&file.path, base))
    })
}

/// Renders a complete selection, refusing paths M3U cannot represent.
///
/// A carriage return or newline in a file name could otherwise inject another
/// entry or a directive. The CLI uses this before publishing any playlist.
pub fn try_render(
    catalog: &Catalog,
    tracks: &[Id],
    base: Option<&Path>,
    style: Style,
) -> Result<String, String> {
    for &id in tracks {
        if let Some(file) = catalog
            .track(id)
            .and_then(|track| catalog.file(track.file_id))
            && relative_to(&file.path, base).contains(['\r', '\n'])
        {
            return Err(format!(
                "{:?}: M3U cannot represent a path containing a line break",
                file.path
            ));
        }
    }
    Ok(render(catalog, tracks, base, style))
}

/// Renders the same metadata with caller-supplied paths. Missing mappings are
/// omitted before their EXTINF is written, as are paths containing line breaks;
/// metadata line breaks become spaces.
pub(crate) fn render_with_paths(
    catalog: &Catalog,
    tracks: &[Id],
    style: Style,
    path: impl Fn(Id) -> Option<String>,
) -> String {
    let mut out = String::new();
    if style == Style::Extended {
        out.push_str("#EXTM3U\n");
    }
    for &id in tracks {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let Some(path) = path(id) else {
            continue;
        };
        if path.contains(['\r', '\n']) {
            continue;
        }
        if style == Style::Extended {
            let artist = catalog
                .credits_on(EntityKind::Track, id)
                .into_iter()
                .find(|(_, role)| *role == "main")
                .map(|(a, _)| a.name.clone())
                .unwrap_or_default();
            // Seconds, rounded, and -1 when unknown: that is what the format
            // says, and a player reading 0 would show a track of no length.
            let seconds = match track.duration_ms {
                Some(ms) => (ms / 1000 + u64::from(ms % 1000 >= 500)) as i64,
                None => -1,
            };
            let title = track.title.replace(['\r', '\n'], " ");
            let artist = artist.replace(['\r', '\n'], " ");
            match artist.is_empty() {
                true => out.push_str(&format!("#EXTINF:{seconds},{title}\n")),
                false => out.push_str(&format!("#EXTINF:{seconds},{artist} - {title}\n")),
            }
        }
        if path.starts_with('#') {
            out.push_str("./");
        }
        out.push_str(&path);
        out.push('\n');
    }
    out
}

/// The path as the playlist should carry it.
fn relative_to(path: &str, base: Option<&Path>) -> String {
    let Some(base) = base.and_then(|b| b.to_str()) else {
        return path.to_string();
    };
    crate::text::relative_under(path, base)
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| path.to_string())
}

/// The name of the playlist a folder should hold: the folder's own name.
///
/// Not the album title, which is tempting and wrong twice over: two folders can
/// hold the same title (a rip and a remaster), and a title carries `/` and `:`
/// on records that were named by people rather than by filesystems. The folder
/// name is unique where the file goes, is already legal there, and is what the
/// user recognises in a player's list.
pub fn file_name(folder: &Path) -> String {
    let stem = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("playlist");
    format!("{stem}.m3u")
}

/// `true` when the file already holds exactly this, byte for byte.
///
/// Content rather than a modification date, because a playlist is derived from
/// the *set* of tracks and not from their bytes: adding a track to an album
/// changes what the playlist should say without touching any file the playlist
/// already names. Comparing the text answers both questions at once, and
/// leaves the file's date alone when nothing changed — which matters to
/// whatever syncs the folder afterwards.
pub fn already_says(path: &Path, content: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|held| held == content)
}

/// Replaces a playlist only after its complete text has been written.
///
/// Shares the exclusive temporary-output policy used by copying. A symbolic
/// link or non-regular final path is refused; failed publication leaves the
/// previous playlist intact and removes unfinished temporary output.
pub fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let output = crate::copy::TemporaryOutput::new(path)?;
    std::fs::write(output.path(), content)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    output.publish()
}

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;
