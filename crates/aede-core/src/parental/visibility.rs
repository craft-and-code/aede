//! Build one compact, derived graph; public identities remain key/path based.
use super::remap::{Maps, compact};
use crate::{
    model::{Catalog, EntityKind},
    user::EntityRef,
};
use std::collections::BTreeSet;

/// Whether an exact public identity resolves inside an already filtered catalogue.
pub fn allows_reference(visible: &Catalog, token: &str) -> bool {
    EntityRef::parse_token(token)
        .filter(|reference| !reference.key.is_empty())
        .and_then(|reference| reference.resolve(visible))
        .is_some()
}
/// Derive a catalogue with restricted artists and all their credited audio removed.
/// The original is never mutated or persisted. Dense identifiers are rebuilt only
/// inside this view, while every surviving public key, path and MBID stays intact.
pub fn filtered_catalog(catalog: &Catalog, blocked_artists: &[String]) -> Result<Catalog, String> {
    let mut maps = Maps::new(catalog)?;
    let mut blocked = BTreeSet::new();
    for token in blocked_artists {
        let reference = EntityRef::parse_token(token).ok_or("invalid blocked artist reference")?;
        if reference.kind != EntityKind::Artist || reference.key.is_empty() {
            return Err("invalid blocked artist reference".into());
        }
        if let Some(id) = reference.resolve(catalog) {
            blocked.insert(id);
        }
    }
    if blocked.is_empty() {
        return Ok(catalog.clone());
    }
    let mut tracks = vec![false; catalog.tracks.len()];
    let mut releases = catalog
        .releases
        .iter()
        .map(|release| {
            release
                .album_artist_id
                .is_some_and(|id| blocked.contains(&id))
        })
        .collect::<Vec<_>>();
    let mut recordings = vec![false; catalog.recordings.len()];
    let mut works = vec![false; catalog.works.len()];
    for credit in &catalog.credits {
        if !blocked.contains(&credit.artist_id) {
            continue;
        }
        let table = match credit.entity_kind {
            EntityKind::Track => &mut tracks,
            EntityKind::Release => &mut releases,
            EntityKind::Recording => &mut recordings,
            EntityKind::Work => &mut works,
            _ => continue,
        };
        *table
            .get_mut(credit.entity_id as usize)
            .ok_or("invalid catalogue credit in parental projection")? = true;
    }
    for recording in &catalog.recordings {
        if recording
            .work_ids
            .iter()
            .any(|id| works.get(*id as usize).copied().unwrap_or(true))
        {
            *recordings
                .get_mut(recording.id as usize)
                .ok_or("invalid catalogue recording")? = true;
        }
    }
    // The same recording placed on a compilation is still the same audio.
    // Removing all of its placements also keeps local recording fallback keys stable.
    for track in &catalog.tracks {
        let denied = tracks
            .get(track.id as usize)
            .copied()
            .ok_or("invalid catalogue track")?
            || track
                .release_id
                .is_some_and(|id| releases.get(id as usize).copied().unwrap_or(true));
        if denied {
            *recordings
                .get_mut(track.recording_id as usize)
                .ok_or("invalid catalogue recording")? = true;
        }
    }
    for track in &catalog.tracks {
        tracks[track.id as usize] = recordings
            .get(track.recording_id as usize)
            .copied()
            .unwrap_or(true);
    }
    for track in catalog
        .tracks
        .iter()
        .filter(|track| !tracks[track.id as usize])
    {
        maps.tracks.keep(track.id)?;
        maps.files.keep(track.file_id)?;
        maps.recordings.keep(track.recording_id)?;
        if let Some(release) = track.release_id {
            maps.releases.keep(release)?;
        }
    }
    for recording in catalog
        .recordings
        .iter()
        .filter(|row| maps.recordings.has(row.id))
    {
        for id in &recording.work_ids {
            maps.works.keep(*id)?;
        }
    }
    for release in catalog
        .releases
        .iter()
        .filter(|row| maps.releases.has(row.id))
    {
        if let Some(id) = release.album_artist_id {
            maps.artists.keep(id)?;
        }
        if let Some(id) = release.release_group_id {
            maps.release_groups.keep(id)?;
        }
        for id in &release.label_ids {
            maps.labels.keep(*id)?;
        }
    }
    for credit in &catalog.credits {
        if !blocked.contains(&credit.artist_id) && maps.kept(credit.entity_kind, credit.entity_id) {
            maps.artists.keep(credit.artist_id)?;
        }
    }
    for link in &catalog.genre_links {
        if maps.kept(link.entity_kind, link.entity_id) {
            maps.genres.keep(link.genre_id)?;
        }
    }
    maps.finish()?;
    compact(catalog, &maps)
}
#[cfg(test)]
#[path = "visibility_tests.rs"]
mod tests;
