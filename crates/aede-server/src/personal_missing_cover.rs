//! Cache reads are offline; only an authenticated explicit POST reaches the archive.
use super::*;
use axum::http::header;
use axum::response::{IntoResponse, Response};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    artist: String,
    release_group: String,
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/missing/cover", get(read).post(fetch))
}
fn select(
    state: &ApiState,
    owner: &PersonalOwner,
    parameters: &Parameters,
) -> Result<(), ApiError> {
    if parameters.artist.len() > 4096
        || !crate::missing_cover_cache::valid_group(&parameters.release_group)
    {
        return Err(invalid_query("invalid missing-album selector"));
    }
    let (_lock, catalog, data) = locked_personal(state)?;
    owner.recheck(state)?;
    let sources = crate::inspection::held_sources(&state.data_dir)?;
    if !rows(&catalog, &sources, &data, &owner.id, true)?
        .iter()
        .any(|row| {
            row["artist"] == parameters.artist && row["release_group"] == parameters.release_group
        })
    {
        return Err(error(
            StatusCode::NOT_FOUND,
            "missing_not_found",
            "no missing album matches this artist and release group",
        ));
    }
    owner.recheck(state)
}
async fn read(
    State(state): State<ApiState>,
    query: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Response, ApiError> {
    respond(state, query, request, false).await
}
async fn fetch(
    State(state): State<ApiState>,
    query: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Response, ApiError> {
    respond(state, query, request, true).await
}
async fn respond(
    state: ApiState,
    query: Result<Query<Parameters>, QueryRejection>,
    request: Request,
    download: bool,
) -> Result<Response, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    if download {
        owner.require_mutation()?;
    }
    empty_body(&body(request).await?)?;
    let Query(parameters) = query.map_err(|failure| invalid_query(failure.body_text()))?;
    if download && !cfg!(feature = "fetch") {
        return Err(error(
            StatusCode::NOT_IMPLEMENTED,
            "fetch_unavailable",
            "archive fetching is disabled in this build",
        ));
    }
    spawn_personal(state.inspection_slots.clone(), move || {
        select(&state, &owner, &parameters)?;
        let image = if download {
            state
                .missing_covers
                .fetch(&state.data_dir, &parameters.release_group)
        } else {
            crate::missing_cover_cache::cached(&state.data_dir, &parameters.release_group)
        }
        .map_err(|failure| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "missing_cover_unavailable",
                failure,
            )
        })?;
        select(&state, &owner, &parameters)?;
        Ok(match image {
            Some(image) => (
                [
                    (
                        header::CONTENT_TYPE,
                        if image.format == "png" {
                            "image/png"
                        } else {
                            "image/jpeg"
                        },
                    ),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                image.bytes,
            )
                .into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        })
    })?
    .await
    .map_err(|_| unavailable())?
}
