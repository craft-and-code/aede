//! Resolve inert playlist entries against catalog identities, without entry I/O.
use crate::model::{Catalog, Id};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

fn normalized(path: &Path) -> Option<PathBuf> {
    if !path.is_absolute() {
        return None;
    }
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    Some(result)
}

/// Preserve order and repetitions, refusing the entire selection on any unknown,
/// ambiguous or non-file entry. Relative paths use the discovered playlist parent.
/// No path is opened, canonicalized or fetched; only current catalog files match.
pub fn resolve(catalog: &Catalog, content: &str, parent: &Path) -> Result<Vec<Id>, String> {
    if content.len() > 8 * 1024 * 1024 || content.contains('\0') || !parent.is_absolute() {
        return Err("invalid or oversized M3U selection".into());
    }
    let mut paths: BTreeMap<PathBuf, Vec<Id>> = BTreeMap::new();
    for track in &catalog.tracks {
        if let Some(file) = catalog.file(track.file_id)
            && let Some(path) = normalized(Path::new(&file.path))
        {
            paths.entry(path).or_default().push(track.id);
        }
    }
    let mut tracks = Vec::new();
    for (index, raw) in content.trim_start_matches('\u{feff}').lines().enumerate() {
        let entry = raw.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        if tracks.len() >= 4096 {
            return Err("M3U selection exceeds 4096 tracks".into());
        }
        // Native paths may contain colons, but URI-shaped entries never become files.
        if entry.contains("://")
            || entry.starts_with("file:")
            || entry.chars().any(char::is_control)
        {
            return Err(format!("M3U line {} is not a catalogued file", index + 1));
        }
        let path = normalized(&parent.join(entry))
            .ok_or_else(|| format!("invalid M3U path on line {}", index + 1))?;
        match paths.get(&path).map(Vec::as_slice) {
            Some([track]) => tracks.push(*track),
            _ => {
                return Err(format!(
                    "M3U line {} has no unique catalogued track; synchronize the library",
                    index + 1
                ));
            }
        }
    }
    if tracks.is_empty() {
        return Err("M3U selection is empty".into());
    }
    Ok(tracks)
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
