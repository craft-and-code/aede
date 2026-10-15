//! Explicit, owner-admitted resolution of existing M3U selections.
use super::*;
use serde_json::{Value, json};

pub(super) async fn library(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    request: Request,
) -> Result<Json<Value>, ApiError> {
    no_query(&request)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    spawn_personal(state.inspection_slots.clone(), move || {
        let scope = files::Scope::capture(&state)?;
        let document = aede_core::playlist::files::read(&scope.roots, &scope.excluded, &id)
            .map_err(files::failure)?;
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let parent = Path::new(&document.file.path)
            .parent()
            .ok_or_else(unavailable)?;
        let ids = aede_core::playlist::selection::resolve(catalog, &document.content, parent)
            .map_err(|message| error(StatusCode::CONFLICT, "playlist_changed", message))?;
        let rows = rows(
            catalog,
            ids.into_iter()
                .map(|id| EntityRef::of(catalog, EntityKind::Track, id).ok_or_else(unavailable))
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        let scanned_at = catalog.scanned_at;
        drop(guard);
        scope.confirm(&state, &owner)?;
        owner.admit(&state, Json(json!({"items":rows,"scanned_at":scanned_at})))
    })?
    .await
    .map_err(|_| unavailable())?
}

pub(super) async fn private(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    request: Request,
) -> Result<Json<Value>, ApiError> {
    no_query(&request)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    target_path(&state.data_dir, &owner.id, &id)?;
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&state)?;
        owner.recheck(&state)?;
        let playlist = data.playlist(&owner.id, &id).ok_or_else(|| error(StatusCode::NOT_FOUND, "playlist_not_found", "no private playlist has this identity"))?;
        owner.admit(&state, Json(json!({"items":rows(&catalog,playlist.tracks.clone())?,"scanned_at":catalog.scanned_at})))
    })?.await.map_err(|_| unavailable())?
}

fn rows(catalog: &Catalog, tracks: Vec<EntityRef>) -> Result<Vec<Value>, ApiError> {
    if tracks.is_empty() || tracks.len() > 4096 {
        return Err(invalid_query("invalid M3U selection size"));
    }
    // One bounded lookup per occurrence; duplicate catalogue placements must not
    // silently select whichever happens to precede the others after a rescan.
    let mut files = std::collections::BTreeMap::new();
    for track in &catalog.tracks {
        if let Some(file) = catalog.file(track.file_id) {
            files
                .entry(file.path.as_str())
                .and_modify(|id| *id = None)
                .or_insert(Some(track.id));
        }
    }
    let mut rows = Vec::with_capacity(tracks.len());
    for track in tracks {
        if track.kind != EntityKind::Track || track.key.len() > MAX_REFERENCE_BYTES - 6 {
            return Err(invalid_query("invalid M3U track"));
        }
        let id = files
            .get(track.key.as_str())
            .copied()
            .flatten()
            .ok_or_else(|| {
                error(
                    StatusCode::CONFLICT,
                    "playlist_changed",
                    "M3U entry has no unique catalogued track; synchronize the library",
                )
            })?;
        rows.push(crate::presentation::playback_track_row(catalog, id)?);
    }
    Ok(rows)
}

#[cfg(test)]
#[path = "personal_m3u_playback_tests.rs"]
mod tests;
