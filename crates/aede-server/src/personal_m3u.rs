//! Native private M3U publication, using canonical playlist rendering and storage.
use super::*;
#[path = "personal_m3u_library.rs"]
mod library;
use library::publish_library;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/me/v1/m3u", get(list).post(save))
        .route("/api/me/v1/m3u/:id", get(download))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    release: String,
    name: String,
    #[serde(default)]
    library: bool,
}
#[derive(Serialize)]
struct Published {
    id: String,
    name: String,
    filename: String,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    library_files: Vec<String>,
}
fn target_path(data_dir: &Path, owner: &str, id: &str) -> Result<PathBuf, ApiError> {
    if owner.is_empty()
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        || !id.starts_with("playlist-")
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(invalid_query("invalid private playlist identity"));
    }
    Ok(data_dir
        .join("playlists")
        .join(owner)
        .join(format!("{id}.m3u")))
}
fn published(catalog: &Catalog, playlist: &user::Playlist) -> Result<Published, ApiError> {
    let tracks = playlist
        .tracks
        .iter()
        .map(|reference| {
            reference
                .resolve(catalog)
                .filter(|_| reference.kind == EntityKind::Track)
                .ok_or_else(|| {
                    error(
                        StatusCode::CONFLICT,
                        "playlist_changed",
                        "a stored track is no longer available",
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let content = aede_core::playlist::try_render(
        catalog,
        &tracks,
        None,
        aede_core::playlist::Style::Extended,
    )
    .map_err(|message| error(StatusCode::BAD_REQUEST, "m3u_refused", message))?;
    if content.len() > 8 * 1024 * 1024 {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "m3u_too_large",
            "playlist is too large",
        ));
    }
    Ok(Published {
        id: playlist.id.clone(),
        name: playlist.name.clone(),
        filename: format!("{}.m3u", playlist.id),
        content,
        library_files: Vec::new(),
    })
}
async fn save(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<Published>, ApiError> {
    no_query(&request)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let publication_admin = require_admin(&state, &request).map(|_| ());
    let input: Selection = json_body(request).await?;
    if input.library {
        publication_admin?;
    }
    validate_collection_name(&input.name)?;
    if input.release.len() > MAX_REFERENCE_BYTES {
        return Err(invalid_query("release reference is too long"));
    }
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, mut data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        let id = crate::catalog::filter_reference(&catalog, &input.release, EntityKind::Release)?;
        let release = catalog.release(id).ok_or_else(unavailable)?;
        let tracks = release
            .track_ids
            .iter()
            .map(|id| EntityRef::of(&catalog, EntityKind::Track, *id).ok_or_else(unavailable))
            .collect::<Result<_, _>>()?;
        let mut answer = save_selection(
            &catalog,
            &mut data,
            &data_dir,
            &owner.id,
            &input.name,
            tracks,
        )?;
        if input.library {
            answer.library_files = publish_library(&catalog, id)?;
        }
        Ok(Json(answer))
    })?
    .await
    .map_err(|_| unavailable())?
}
// Repeated explicit saves restore the same retained selection instead of consuming playlist slots.
fn save_selection(
    catalog: &Catalog,
    data: &mut UserData,
    data_dir: &Path,
    owner: &str,
    name: &str,
    tracks: Vec<EntityRef>,
) -> Result<Published, ApiError> {
    let retained = data
        .playlists
        .iter()
        .rev()
        .find(|p| p.owner == owner && p.name == name && p.tracks == tracks)
        .map(|p| p.id.clone());
    let existing = retained.is_some();
    let id = match retained {
        Some(id) => id,
        None => data
            .create_playlist(owner, name, tracks, clock::now_seconds())
            .map_err(|message| error(StatusCode::BAD_REQUEST, "playlist_refused", message))?,
    };
    let answer = published(catalog, data.playlist(owner, &id).ok_or_else(unavailable)?)?;
    let path = target_path(data_dir, owner, &id)?;
    let folder = path.parent().ok_or_else(unavailable)?;
    std::fs::create_dir_all(folder).map_err(|_| unavailable())?;
    if !std::fs::symlink_metadata(folder).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
        return Err(unavailable());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| unavailable())?;
    }
    let created =
        store::write_new_atomic(&path, answer.content.as_bytes()).map_err(|_| unavailable())?;
    if !created {
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| unavailable())?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != answer.content.len() as u64
            || {
                use std::io::Read;
                let mut held = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|_| unavailable())?
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut held)
                    .map_err(|_| unavailable())?;
                held != answer.content.as_bytes()
            }
        {
            return Err(error(
                StatusCode::CONFLICT,
                "playlist_exists",
                "existing playlist file differs; it has not been overwritten",
            ));
        }
    }
    if !existing && user::save(data, &user::user_path(data_dir)).is_err() {
        if created {
            let _ = std::fs::remove_file(path);
        }
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "private playlist could not be saved",
        ));
    }
    Ok(answer)
}
async fn list(
    State(state): State<ApiState>,
    input: Result<Query<PageQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<Page<Summary>>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let (offset, limit) = page(parsed_query(input)?)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        let own: Vec<_> = data
            .playlists
            .iter()
            .filter(|playlist| playlist.owner == owner.id)
            .collect();
        let items = own
            .iter()
            .skip(offset)
            .take(limit)
            .map(|playlist| Summary {
                id: playlist.id.clone(),
                name: playlist.name.clone(),
                track_count: playlist.tracks.len(),
            })
            .collect();
        Ok(Json(Page::new(
            items,
            own.len(),
            offset,
            limit,
            catalog.scanned_at,
        )))
    })?
    .await
    .map_err(|_| unavailable())?
}
#[derive(Serialize)]
struct Summary {
    id: String,
    name: String,
    track_count: usize,
}
async fn download(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    request: Request,
) -> Result<Json<Published>, ApiError> {
    no_query(&request)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let data_dir = state.data_dir.clone();
    target_path(&data_dir, &owner.id, &id)?;
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        let playlist = data.playlist(&owner.id, &id).ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "playlist_not_found",
                "no private playlist has this identity",
            )
        })?;
        published(&catalog, playlist).map(Json)
    })?
    .await
    .map_err(|_| unavailable())?
}

#[cfg(test)]
#[path = "personal_m3u_tests.rs"]
mod tests;
