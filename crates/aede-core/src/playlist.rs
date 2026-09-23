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
pub fn render(catalog: &Catalog, tracks: &[Id], base: Option<&Path>, style: Style) -> String {
    let mut out = String::new();
    if style == Style::Extended {
        out.push_str("#EXTM3U\n");
    }
    for &id in tracks {
        let Some(track) = catalog.track(id) else {
            continue;
        };
        let Some(file) = catalog.file(track.file_id) else {
            continue;
        };
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
                Some(ms) => ((ms + 500) / 1000) as i64,
                None => -1,
            };
            match artist.is_empty() {
                true => out.push_str(&format!("#EXTINF:{seconds},{}\n", track.title)),
                false => out.push_str(&format!("#EXTINF:{seconds},{artist} - {}\n", track.title)),
            }
        }
        out.push_str(&relative_to(&file.path, base));
        out.push('\n');
    }
    out
}

/// The path as the playlist should carry it.
fn relative_to(path: &str, base: Option<&Path>) -> String {
    let Some(base) = base.and_then(|b| b.to_str()) else {
        return path.to_string();
    };
    let prefix = match base.ends_with('/') {
        true => base.to_string(),
        false => format!("{base}/"),
    };
    path.strip_prefix(&prefix).unwrap_or(path).to_string()
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

#[cfg(test)]
#[path = "playlist_tests.rs"]
mod tests;
