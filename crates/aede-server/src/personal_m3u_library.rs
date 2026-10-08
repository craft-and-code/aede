//! Explicit local-administrator publication uses the CLI's shared folder plan.
use super::*;
use aede_core::playlist::{self, Style, folders};
use std::collections::BTreeMap;

pub(super) fn publish_library(
    catalog: &Catalog,
    release_id: aede_core::model::Id,
) -> Result<Vec<String>, ApiError> {
    let release = catalog.release(release_id).ok_or_else(unavailable)?;
    let folder = PathBuf::from(&release.folder);
    let mut selected = BTreeMap::new();
    let mut albums = folders::albums(catalog, &[]);
    selected.insert(
        folder.clone(),
        albums.remove(&folder).ok_or_else(unavailable)?,
    );
    if let Some(artist) = release.album_artist_id {
        let releases: Vec<_> = catalog
            .releases
            .iter()
            .filter(|r| r.album_artist_id == Some(artist))
            .collect();
        if let Some(artist_folder) = aede_core::coverart::shared_artist_folder(&releases)
            && let Some(tracks) = folders::discographies(catalog, &[]).remove(&artist_folder)
        {
            selected.entry(artist_folder).or_default().extend(tracks);
        }
    }
    let planned = folders::render_folders(catalog, selected, Style::Extended)
        .map_err(|_| invalid_query("M3U cannot represent this selection"))?;
    if planned.is_empty() {
        return Err(invalid_query("no album tracks to publish"));
    }
    for (path, content) in &planned {
        let parent = path
            .parent()
            .ok_or_else(unavailable)?
            .canonicalize()
            .map_err(|_| unavailable())?;
        if !catalog.roots.iter().any(|root| {
            Path::new(root)
                .canonicalize()
                .is_ok_and(|root| parent.starts_with(root))
        }) || content.len() > 8 * 1024 * 1024
        {
            return Err(invalid_query(
                "playlist destination is outside the available library",
            ));
        }
    }
    let mut files = Vec::new();
    for (path, content) in planned {
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(error(
                StatusCode::CONFLICT,
                "playlist_write_failed",
                "library playlist destination is not a regular file",
            ));
        }
        if !playlist::already_says(&path, &content) {
            playlist::write_atomic(&path, &content).map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "playlist_write_failed",
                    "library playlist could not be published",
                )
            })?;
        }
        files.push(path.to_string_lossy().into_owned());
    }
    Ok(files)
}
