//! Authenticated read-only access to playlists already present in watched roots.

use super::*;
use aede_core::playlist::files as library_files;
#[path = "personal_m3u_album.rs"]
mod album;
#[path = "personal_m3u_files_response.rs"]
mod response;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/me/v1/m3u-files", get(list))
        .route("/api/me/v1/m3u-files/:id", get(read))
        .route(
            "/api/me/v1/m3u-files/:id/playback",
            get(super::playback::library),
        )
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    offset: Option<String>,
    limit: Option<String>,
    q: Option<String>,
    #[serde(rename = "ref")]
    reference: Option<String>,
}

pub(super) fn parameters(query: Parameters) -> Result<(usize, usize, Option<String>), ApiError> {
    let (offset, limit) = page_bounds(query.offset.as_deref(), query.limit.as_deref())?;
    let q = query
        .q
        .map(|q| {
            bounded_text(&q, "q", MAX_SEARCH_TEXT_BYTES)?;
            let normalized = text::normalize(&q);
            if normalized.is_empty() {
                Err(invalid_query("q must contain searchable text"))
            } else {
                Ok(normalized)
            }
        })
        .transpose()?;
    Ok((offset, limit, q))
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Scope {
    pub(super) roots: Vec<String>,
    pub(super) excluded: Vec<String>,
    scanned_at: u64,
    canonical_paths: Vec<Option<PathBuf>>,
}

impl Scope {
    pub(super) fn capture(state: &ApiState) -> Result<Self, ApiError> {
        let mut scope = {
            let catalog = state.catalog.blocking_read();
            let catalog = catalog.as_ref().ok_or_else(unavailable)?;
            Self {
                roots: catalog.roots.clone(),
                excluded: catalog.excluded.clone(),
                scanned_at: catalog.scanned_at,
                canonical_paths: Vec::new(),
            }
        };
        // An encompassing watched root must never turn any account's retained
        // private exports into shared library files, even with a computable ID.
        let private_exports = state.data_dir.join("playlists");
        scope.excluded.push(
            private_exports
                .to_str()
                .ok_or_else(|| failure(library_files::Error::InvalidPath))?
                .to_owned(),
        );
        // Filesystem operations run after releasing the catalog guard. Unchanged
        // root strings do not authorize publication after an alias is retargeted.
        scope.canonical_paths = scope
            .roots
            .iter()
            .chain(&scope.excluded)
            .map(|path| Path::new(path).canonicalize().ok())
            .collect();
        Ok(scope)
    }

    pub(super) fn confirm(&self, state: &ApiState, owner: &PersonalOwner) -> Result<(), ApiError> {
        if *self != Self::capture(state)? {
            return Err(failure(library_files::Error::Changed));
        }
        owner.recheck(state)
    }
}

#[derive(Serialize)]
struct Summary {
    id: String,
    name: String,
    filename: String,
    path: String,
    track_count: Option<usize>,
}
impl From<library_files::File> for Summary {
    fn from(file: library_files::File) -> Self {
        Self {
            id: file.id,
            name: file.name,
            filename: file.filename,
            path: file.path,
            track_count: None,
        }
    }
}

#[derive(Serialize)]
struct Listing {
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<String>,
    #[serde(flatten)]
    page: Page<Summary>,
    unavailable_roots: usize,
    unreadable_directories: usize,
}

pub(super) fn failure(reason: library_files::Error) -> ApiError {
    use library_files::Error;
    let (status, code, message) = match reason {
        Error::InvalidIdentity => (
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "invalid M3U file identity",
        ),
        Error::NotFound => (
            StatusCode::NOT_FOUND,
            "m3u_file_not_found",
            "no permitted M3U file has this identity",
        ),
        Error::Changed => (
            StatusCode::CONFLICT,
            "m3u_files_changed",
            "the file or watched roots changed; refresh the list",
        ),
        Error::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "m3u_file_unavailable",
            "the selected M3U file could not be read",
        ),
        Error::InvalidPath => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "m3u_path_invalid",
            "a playlist path is not bounded lossless UTF-8",
        ),
        Error::TooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "m3u_files_too_large",
            "the M3U traversal, content, entry or response limit was exceeded",
        ),
        Error::Timeout => (
            StatusCode::SERVICE_UNAVAILABLE,
            "m3u_files_timeout",
            "the M3U inspection deadline was reached",
        ),
        Error::InvalidText => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "m3u_text_invalid",
            "the selected file is not UTF-8 M3U text",
        ),
    };
    error(status, code, message)
}

async fn list(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Response, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let query = parsed_query(input)?;
    let reference = query.reference.clone();
    let (offset, limit, q) = parameters(query)?;
    let bytes = spawn_personal(state.inspection_slots.clone(), move || {
        let scope = Scope::capture(&state)?;
        let (roots, target) = match reference.as_deref() {
            Some(reference) => album::target(&state, &scope, reference)?,
            None => (scope.roots.clone(), None),
        };
        let discovery = library_files::discover(&roots, &scope.excluded).map_err(failure)?;
        let mut files = discovery.files;
        files.retain(|file| {
            target
                .as_ref()
                .is_none_or(|target| Path::new(&file.path) == target)
                && q.as_ref()
                    .is_none_or(|q| text::normalize(&file.path).contains(q))
        });
        let total = files.len();
        let items = files
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(Summary::from)
            .collect();
        let answer = Listing {
            reference: reference.clone(),
            page: Page::new(items, total, offset, limit, scope.scanned_at),
            unavailable_roots: discovery.unavailable_roots,
            unreadable_directories: discovery.unreadable_directories,
        };
        let bytes = response::serialize(&answer)?;
        if let Some(reference) = reference.as_deref()
            && album::target(&state, &scope, reference)? != (roots, target)
        {
            return Err(failure(library_files::Error::Changed));
        }
        scope.confirm(&state, &owner)?;
        Ok(bytes)
    })?
    .await
    .map_err(|_| unavailable())??;
    Ok(response::json(bytes))
}

#[derive(Serialize)]
struct Detail {
    #[serde(flatten)]
    file: Summary,
    content: String,
}

async fn read(
    State(state): State<ApiState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    no_query(&request)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let bytes = spawn_personal(state.inspection_slots.clone(), move || {
        let scope = Scope::capture(&state)?;
        let document = library_files::read(&scope.roots, &scope.excluded, &id).map_err(failure)?;
        let mut file = Summary::from(document.file);
        file.track_count = Some(document.track_count);
        let bytes = response::serialize(&Detail {
            file,
            content: document.content,
        })?;
        scope.confirm(&state, &owner)?;
        Ok(bytes)
    })?
    .await
    .map_err(|_| unavailable())??;
    Ok(response::json(bytes))
}

#[cfg(test)]
#[path = "personal_m3u_files_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "personal_m3u_files_privacy_tests.rs"]
mod privacy_tests;
