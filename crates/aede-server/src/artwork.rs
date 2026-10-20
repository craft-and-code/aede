//! Native bearer artwork uses the same bounded descriptor checks as Subsonic.
use super::*;
use crate::subsonic::artwork::{Source, available, candidate, read_image};
use aede_core::coverart::{self, Kind};

pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/artwork", get(read))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    #[serde(rename = "ref")]
    reference: String,
    kind: String,
    size: Option<u32>,
}

pub(super) fn artist_folder(catalog: &Catalog, id: Id, data_dir: &Path) -> Option<PathBuf> {
    let artist = catalog.artist(id)?;
    let releases: Vec<_> = catalog
        .releases
        .iter()
        .filter(|release| release.album_artist_id == Some(id))
        .collect();
    let mbid = artist.mbid.as_deref().filter(|value| {
        value.len() == 36
            && value.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            })
    });
    if let Some(folder) = coverart::shared_artist_folder(&releases)
        && !catalog
            .roots
            .iter()
            .any(|root| root == folder.to_string_lossy().as_ref())
    {
        return Some(folder);
    }
    mbid.and_then(|mbid| coverart::artist_destination(catalog, &releases, data_dir, mbid))
}

/// Shared artwork can be stored under a confirmed provider ID even when local
/// tags have no MBID or still use a verified merged alias. Keep existing local
/// images first, and retain both confirmed cache locations after retagging.
pub(super) fn artist_folders(
    catalog: &Catalog,
    sources: &aede_core::sources::Sources,
    id: Id,
    data_dir: &Path,
) -> Vec<PathBuf> {
    let mut folders: Vec<_> = artist_folder(catalog, id, data_dir).into_iter().collect();
    let Some(entity) = EntityRef::of(catalog, EntityKind::Artist, id) else {
        return folders;
    };
    let Some(record) = sources
        .get(&entity, aede_core::sources::MUSICBRAINZ)
        .filter(|record| sources.is_trusted(catalog, record))
    else {
        return folders;
    };
    for identifier in [
        record.source_id.as_deref(),
        record.musicbrainz_lookup_alias(),
    ]
    .into_iter()
    .flatten()
    .filter(|id| aede_core::musicbrainz::valid_id(id))
    {
        if let Some(folder) = coverart::asset_destination(data_dir, EntityKind::Artist, identifier)
            && !folders.contains(&folder)
        {
            folders.push(folder);
        }
    }
    folders
}

pub(super) fn beside(folder: PathBuf, kind: Kind) -> Option<Source> {
    ["jpg", "png"]
        .into_iter()
        .map(|format| folder.join(coverart::image_name(kind, format, 0, 1)))
        .find_map(|path| {
            let source = Source {
                path,
                folder: folder.clone(),
            };
            available(&source).then_some(source)
        })
}

fn select(catalog: &Catalog, data_dir: &Path, parameters: &Parameters) -> Result<Source, ApiError> {
    let entity = EntityRef::parse_token(&parameters.reference)
        .ok_or_else(|| invalid_query("invalid artwork reference"))?;
    let id = entity.resolve(catalog).ok_or_else(unavailable)?;
    let selected = match (entity.kind, parameters.kind.as_str()) {
        (EntityKind::Release, "cover") => catalog.release(id).and_then(candidate),
        (EntityKind::Artist, "banner" | "background" | "portrait" | "logo") => {
            let kind = match parameters.kind.as_str() {
                "banner" => Kind::Banner,
                "background" => Kind::Background,
                "logo" => Kind::Logo,
                _ => Kind::Artist,
            };
            // Ordinary sidecars need no source-store read. On a cache miss,
            // use the same trusted provider identities as the availability view.
            if let Some(source) =
                artist_folder(catalog, id, data_dir).and_then(|folder| beside(folder, kind))
            {
                Some(source)
            } else {
                let sources = crate::inspection::held_sources(data_dir)?;
                artist_folders(catalog, &sources, id, data_dir)
                    .into_iter()
                    .find_map(|folder| beside(folder, kind))
            }
        }
        (EntityKind::Track, "spectrum") => catalog
            .track(id)
            .and_then(|track| catalog.file(track.file_id))
            .and_then(|file| {
                let audio = PathBuf::from(&file.path);
                let folder = audio.parent()?.to_path_buf();
                Some(Source {
                    path: aede_core::spectrum::picture_for(&audio),
                    folder,
                })
            }),
        _ => return Err(invalid_query("unsupported artwork kind for this entity")),
    };
    selected.ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            "artwork_unavailable",
            "no local artwork is available",
        )
    })
}

async fn read(
    State(state): State<ApiState>,
    query: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Response, ApiError> {
    let Query(parameters) = query.map_err(|failure| invalid_query(failure.body_text()))?;
    if parameters.reference.len() > MAX_REFERENCE_BYTES
        || parameters.size.is_some_and(|size| size == 0 || size > 2048)
    {
        return Err(invalid_query("invalid artwork reference or size"));
    }
    let identity = auth::SocketIdentity {
        session: request.extensions().get::<auth::Principal>().cloned(),
        administrative: request
            .extensions()
            .get::<auth::AdministrativeAccess>()
            .is_some(),
    };
    let transfer = state
        .playback_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            error(
                StatusCode::TOO_MANY_REQUESTS,
                "artwork_busy",
                "artwork transfers are busy",
            )
        })?;
    let for_worker = state.clone();
    let Json((rendered, transfer)) = tokio::time::timeout(
        Duration::from_secs(10),
        inspection::inspect(for_worker, move |state| {
            let guard = state.catalog.blocking_read();
            let catalog = guard.as_ref().ok_or_else(unavailable)?;
            let source = select(catalog, &state.data_dir, &parameters)?;
            drop(guard);
            let rendered = read_image(source, parameters.size).map_err(|_| {
                error(
                    StatusCode::CONFLICT,
                    "artwork_changed",
                    "artwork is unavailable, unsupported or changed",
                )
            })?;
            auth::confirm_access(state, &identity)?;
            // Cancellation/timeout drops the HTTP waiter, not this blocking
            // work. Keep both its transfer slot and its eventual response
            // bytes covered until the real worker or consumer releases them.
            Ok((rendered, transfer))
        }),
    )
    .await
    .map_err(|_| {
        error(
            StatusCode::REQUEST_TIMEOUT,
            "artwork_timeout",
            "artwork rendering did not finish",
        )
    })??;
    let mime = if rendered.format == "jpg" {
        "image/jpeg"
    } else {
        "image/png"
    };
    // Keep the transfer admission permit until the response bytes are consumed.
    let bytes = hyper::body::Bytes::from(rendered.bytes);
    let length = bytes.len();
    let body = ArtworkBody {
        bytes: Some(bytes),
        _transfer: transfer,
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, length)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(axum::body::Body::new(body))
        .map_err(|_| unavailable())
}

struct ArtworkBody {
    bytes: Option<hyper::body::Bytes>,
    _transfer: tokio::sync::OwnedSemaphorePermit,
}
impl hyper::body::Body for ArtworkBody {
    type Data = hyper::body::Bytes;
    type Error = std::convert::Infallible;
    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        std::task::Poll::Ready(
            self.bytes
                .take()
                .map(|bytes| Ok(hyper::body::Frame::data(bytes))),
        )
    }
    fn is_end_stream(&self) -> bool {
        self.bytes.is_none()
    }
    fn size_hint(&self) -> hyper::body::SizeHint {
        hyper::body::SizeHint::with_exact(self.bytes.as_ref().map_or(0, |bytes| bytes.len() as u64))
    }
}

#[cfg(test)]
#[path = "artwork_tests.rs"]
mod tests;
