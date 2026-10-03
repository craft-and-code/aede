//! Destination playlists retain only selected audio and use its planned paths.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{Item, ItemKind, Plan};
use crate::model::{Catalog, Id};

impl Plan {
    /// Rewrites selected M3U/M3U8 companions and adds a selection M3U8.
    ///
    /// Source playlists are read as UTF-8 and left untouched. Entries not in
    /// this plan (including remote URLs) are omitted; the returned count makes
    /// that narrowing visible. Paths refer to adapted names and converted
    /// extensions, relative to each destination playlist. The generated
    /// selection follows the caller's track order.
    pub fn prepare_playlists(&mut self, catalog: &Catalog, tracks: &[Id]) -> Result<usize, String> {
        let mapping: BTreeMap<String, PathBuf> = self
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Audio)
            .map(|item| {
                (
                    path_key(&item.source.to_string_lossy()),
                    item.relative.clone(),
                )
            })
            .collect();
        let mut omitted = 0;
        for item in &mut self.items {
            if item.kind == ItemKind::Audio || !is_playlist(&item.source) {
                continue;
            }
            let text = std::fs::read_to_string(&item.source).map_err(|error| {
                format!(
                    "{}: cannot read playlist as UTF-8: {error}",
                    item.source.display()
                )
            })?;
            let (rendered, removed) = remap(&text, Some(&item.source), &item.relative, &mapping)?;
            omitted += removed;
            item.size = rendered.len() as u64;
            item.contents = Some(rendered);
        }
        let mut root_names: BTreeSet<String> = self
            .items
            .iter()
            .filter_map(|item| item.relative.components().next())
            .map(|component| super::names::portable_key(&component.as_os_str().to_string_lossy()))
            .collect();
        let relative = PathBuf::from(super::names::make_unique_portable(
            "aede-selection.m3u8",
            &mut root_names,
        ));
        let mut paths = BTreeMap::new();
        for &id in tracks {
            let Some(file) = catalog
                .track(id)
                .and_then(|track| catalog.file(track.file_id))
            else {
                continue;
            };
            let Some(path) = mapping.get(&path_key(&file.path)) else {
                continue;
            };
            let path = relative_path(Path::new(""), path);
            if path.contains(['\n', '\r']) {
                return Err("selected output path cannot be represented in M3U".into());
            }
            let path = if path.starts_with('#') {
                format!("./{path}")
            } else {
                path
            };
            paths.insert(id, path);
        }
        let contents = crate::playlist::render_with_paths(
            catalog,
            tracks,
            crate::playlist::Style::Extended,
            |id| paths.get(&id).cloned(),
        );
        self.items.push(Item {
            source: PathBuf::from("selected tracks"),
            relative,
            size: contents.len() as u64,
            kind: ItemKind::Other,
            convert: None,
            contents: Some(contents),
        });
        Ok(omitted)
    }
}

fn is_playlist(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        let extension = extension.to_string_lossy();
        extension.eq_ignore_ascii_case("m3u") || extension.eq_ignore_ascii_case("m3u8")
    })
}

fn remap(
    text: &str,
    source: Option<&Path>,
    output: &Path,
    mapping: &BTreeMap<String, PathBuf>,
) -> Result<(String, usize), String> {
    let mut rendered = String::new();
    let mut pending = Vec::new();
    let mut omitted = 0;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line == "#EXTM3U" {
            rendered.push_str("#EXTM3U\n");
            continue;
        }
        if line.starts_with('#') {
            pending.push(line);
            continue;
        }
        let key = match source {
            Some(source) if !is_absolute(line) => path_key(&format!(
                "{}/{}",
                crate::text::folder(&source.to_string_lossy()),
                line
            )),
            _ => path_key(line),
        };
        let Some(target) = mapping.get(&key) else {
            pending.clear();
            omitted += 1;
            continue;
        };
        for comment in pending.drain(..) {
            rendered.push_str(comment);
            rendered.push('\n');
        }
        let path = relative_path(output.parent().unwrap_or_else(|| Path::new("")), target);
        if path.contains(['\n', '\r']) {
            return Err(format!(
                "{}: output path cannot be represented in M3U",
                target.display()
            ));
        }
        if path.starts_with('#') {
            rendered.push_str("./");
        }
        rendered.push_str(&path);
        rendered.push('\n');
    }
    Ok((rendered, omitted))
}

fn is_absolute(path: &str) -> bool {
    path.starts_with('/')
        || path.starts_with("\\\\")
        || (path.as_bytes().get(1) == Some(&b':')
            && path
                .as_bytes()
                .get(2)
                .is_some_and(|separator| matches!(separator, b'/' | b'\\')))
}

/// Lexical normalization also supports Windows source playlists on Unix test
/// hosts. Backslashes are separators only for a Windows absolute spelling.
fn path_key(path: &str) -> String {
    let windows = path.starts_with("\\\\")
        || (path.as_bytes().get(1) == Some(&b':')
            && path
                .as_bytes()
                .get(2)
                .is_some_and(|separator| matches!(separator, b'/' | b'\\')));
    let normalized = if windows {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let key = format!(
        "{}{}",
        if normalized.starts_with('/') { "/" } else { "" },
        parts.join("/")
    );
    if windows { key.to_lowercase() } else { key }
}

fn relative_path(base: &Path, target: &Path) -> String {
    let base: Vec<_> = base.components().collect();
    let target: Vec<_> = target.components().collect();
    let common = base
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    let mut out = vec!["..".to_string(); base.len() - common];
    out.extend(
        target[common..]
            .iter()
            .map(|component| component.as_os_str().to_string_lossy().into_owned()),
    );
    out.join("/")
}

#[cfg(test)]
#[path = "playlists_tests.rs"]
mod tests;
