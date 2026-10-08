//! Shared album/discography destinations for the CLI and explicit server publication.
use super::Style;
use crate::model::{Catalog, Id};
use std::collections::BTreeMap;
use std::path::PathBuf;
fn in_scope(path: &str, scope: &[String]) -> bool {
    scope.is_empty() || scope.iter().any(|root| crate::text::is_under(path, root))
}
fn lines(text: &str) -> usize {
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count()
}
/// One playlist per album folder, holding that album in its own order.
pub fn albums(catalog: &Catalog, scope: &[String]) -> BTreeMap<PathBuf, Vec<Id>> {
    let mut by_folder: BTreeMap<PathBuf, Vec<Id>> = BTreeMap::new();
    for release in &catalog.releases {
        if !in_scope(&release.folder, scope) {
            continue;
        }
        // A box set laid out as `Album/Disc 1`, `Album/Disc 2` is *one*
        // release living in the parent, so its playlist goes there and spans
        // the discs — which is exactly the file somebody wants when the tracks
        // are numbered 1..17 twice over.
        let folder = PathBuf::from(&release.folder);
        by_folder
            .entry(folder)
            .or_default()
            .extend(&release.track_ids);
    }
    by_folder
}

/// Render one deduplicated relative-path playlist per physical folder.
pub fn render_folders(
    catalog: &Catalog,
    folders: BTreeMap<PathBuf, Vec<Id>>,
    style: Style,
) -> Result<Vec<(PathBuf, String)>, String> {
    let mut out = Vec::new();
    for (folder, mut tracks) in folders {
        let mut seen = std::collections::BTreeSet::new();
        tracks.retain(|id| seen.insert(*id));
        let text = super::try_render(catalog, &tracks, Some(&folder), style)?;
        if lines(&text) == 0 {
            continue;
        }
        out.push((folder.join(super::file_name(&folder)), text));
    }
    Ok(out)
}

/// One playlist per artist folder, holding every album of that artist in order.
///
/// The artist folder is not a thing the catalog holds — it is inferred as the
/// folder every one of that artist's albums sits in. Where they do not share
/// one, or share only a watched root, nothing is written: a library laid out
/// flat would otherwise get one playlist per artist dumped in its root, which
/// is not tidying but littering.
pub fn discographies(catalog: &Catalog, scope: &[String]) -> BTreeMap<PathBuf, Vec<Id>> {
    let mut by_artist: BTreeMap<Id, Vec<&crate::model::Release>> = BTreeMap::new();
    for release in &catalog.releases {
        if let Some(artist) = release.album_artist_id
            && in_scope(&release.folder, scope)
        {
            by_artist.entry(artist).or_default().push(release);
        }
    }

    let roots: Vec<&str> = catalog.roots.iter().map(String::as_str).collect();
    let mut by_folder: BTreeMap<PathBuf, Vec<&crate::model::Release>> = BTreeMap::new();
    for (_, releases) in by_artist {
        let Some(folder) = crate::coverart::shared_artist_folder(&releases) else {
            continue;
        };
        let name = folder.to_string_lossy().to_string();
        // The root of a library is not an artist folder, and neither is a
        // folder that *is* one of the album folders — that one already has its
        // own playlist under the very same name.
        if roots.contains(&name.as_str()) || releases.iter().any(|r| r.folder == name) {
            continue;
        }
        by_folder.entry(folder).or_default().extend(releases);
    }
    by_folder
        .into_iter()
        .map(|(folder, mut releases)| {
            // Chronological, which is the order a discography is read in; the
            // title breaks a tie so that two records of the same year come out the
            // same way on every machine.
            releases.sort_by(|a, b| {
                a.year
                    .unwrap_or(0)
                    .cmp(&b.year.unwrap_or(0))
                    .then_with(|| a.title.cmp(&b.title))
            });
            let tracks: Vec<Id> = releases.iter().flat_map(|r| r.track_ids.clone()).collect();
            (folder, tracks)
        })
        .collect()
}

#[cfg(test)]
#[path = "folders_tests.rs"]
mod tests;
