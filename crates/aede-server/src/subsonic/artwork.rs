//! Catalogued sidecar artwork, with response-only thumbnails and no writes.

use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Component, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aede_core::coverart;
use aede_core::model::{Catalog, EntityKind, Release};
use aede_core::user::EntityRef;
use axum::http::header;
use axum::response::Response;
use hyper::body::Bytes;

use super::authentication::{self, Identity};
use super::{Parameters, ProtocolError, media, protocol};
use crate::ApiState;

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const WORKER_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Source {
    path: PathBuf,
    folder: PathBuf,
}

fn candidate(release: &Release) -> Option<Source> {
    let path = PathBuf::from(release.cover_path.as_deref()?);
    let folder = PathBuf::from(&release.folder);
    let extension = path.extension()?.to_str()?;
    if !["jpg", "jpeg", "png"]
        .iter()
        .any(|known| extension.eq_ignore_ascii_case(known))
        || !path.is_absolute()
        || !folder.is_absolute()
        || [&path, &folder].iter().any(|path| {
            path.components()
                .any(|part| matches!(part, Component::ParentDir))
        })
        || path == folder
        || !path.starts_with(&folder)
    {
        return None;
    }
    Some(Source { path, folder })
}

/// Pure projection of a dedicated cover ID; no paths or filesystem reads.
/// Only catalogued JPEG/PNG sidecars within the release folder are advertised.
pub(super) fn cover_id(
    catalog: &Catalog,
    release: &Release,
) -> Result<Option<String>, ProtocolError> {
    if candidate(release).is_none() {
        return Ok(None);
    }
    let reference =
        EntityRef::of(catalog, EntityKind::Release, release.id).ok_or_else(unavailable)?;
    Ok(Some(format!(
        "cover-{}",
        protocol::sha256(&reference.to_token())?
    )))
}

fn unavailable() -> ProtocolError {
    ProtocolError::new(70, "cover artwork is unavailable, unsupported or changed")
}

fn source(catalog: &Catalog, wanted: &str) -> Result<Source, ProtocolError> {
    if wanted.len() != 70 || !wanted.starts_with("cover-") {
        return Err(unavailable());
    }
    let mut found = None;
    for release in &catalog.releases {
        if cover_id(catalog, release)?.as_deref() == Some(wanted) {
            if found.is_some() {
                return Err(ProtocolError::new(0, "cover identities are ambiguous"));
            }
            found = candidate(release);
        }
    }
    found.ok_or_else(unavailable)
}

fn regular(metadata: &Metadata) -> bool {
    metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= MAX_BYTES
}

fn unchanged(before: &Metadata, after: &Metadata) -> bool {
    regular(after)
        && media::same_file(before, after)
        && before.len() == after.len()
        && matches!((before.modified(), after.modified()), (Ok(before), Ok(after)) if before == after)
}

fn confined(source: &Source) -> Result<(), ProtocolError> {
    let folder = fs::symlink_metadata(&source.folder).map_err(|_| unavailable())?;
    if !folder.is_dir() || folder.file_type().is_symlink() {
        return Err(unavailable());
    }
    let folder = fs::canonicalize(&source.folder).map_err(|_| unavailable())?;
    let path = fs::canonicalize(&source.path).map_err(|_| unavailable())?;
    if path == folder || !path.starts_with(folder) {
        return Err(unavailable());
    }
    Ok(())
}

fn read_image(source: Source, size: Option<u32>) -> Result<coverart::RenderedImage, ProtocolError> {
    confined(&source)?;
    let before = fs::symlink_metadata(&source.path).map_err(|_| unavailable())?;
    if !regular(&before) {
        return Err(unavailable());
    }
    let mut file = File::open(&source.path).map_err(|_| unavailable())?;
    let opened = file.metadata().map_err(|_| unavailable())?;
    if !unchanged(&before, &opened) {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(opened.len() as usize)
        .map_err(|_| unavailable())?;
    (&mut file)
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() as u64 != opened.len()
        || !unchanged(&opened, &file.metadata().map_err(|_| unavailable())?)
    {
        return Err(unavailable());
    }
    let rendered = coverart::render_image(&bytes, size).map_err(|_| unavailable())?;
    if !unchanged(&opened, &file.metadata().map_err(|_| unavailable())?)
        || !unchanged(
            &opened,
            &fs::symlink_metadata(&source.path).map_err(|_| unavailable())?,
        )
    {
        return Err(unavailable());
    }
    confined(&source)?;
    Ok(rendered)
}

/// Return verified artwork for any current account, including an auditor.
/// CPU/I/O and response memory retain their shared admission permits.
pub(super) async fn response(
    state: ApiState,
    identity: Identity,
    parameters: Parameters,
) -> Result<Response, ProtocolError> {
    parameters.allowed(&["id", "size"])?;
    let wanted = parameters.required("id")?.to_string();
    let size = if parameters.get("size").is_some() {
        let size = parameters.number("size", 0, 2048)?;
        if size == 0 {
            return Err(ProtocolError::new(
                10,
                "size must be between 1 and 2048 pixels",
            ));
        }
        Some(size as u32)
    } else {
        None
    };
    let mut shutdown = state.shutdown.subscribe();
    let selected = tokio::select! {
        _ = shutdown.recv() => return Err(ProtocolError::new(0, "server is shutting down")),
        selected = super::inspect(state.clone(), move |catalog| source(catalog, &wanted)) => selected?,
    };
    let transfer = state
        .playback_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ProtocolError::new(0, "too many active media transfers; retry later"))?;
    let worker = state
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ProtocolError::new(0, "artwork work is busy; retry later"))?;
    let reading = tokio::time::timeout(
        WORKER_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let _worker = worker;
            let transfer = Arc::new(transfer);
            read_image(selected, size).map(|rendered| (rendered, transfer))
        }),
    );
    let (rendered, transfer) = tokio::select! {
        _ = shutdown.recv() => return Err(ProtocolError::new(0, "server is shutting down")),
        rendered = reading => rendered.map_err(|_| unavailable())?.map_err(|_| unavailable())??,
    };
    tokio::select! {
        _ = shutdown.recv() => return Err(ProtocolError::new(0, "server is shutting down")),
        confirmed = tokio::time::timeout(AUTH_TIMEOUT, authentication::recheck(&state, &identity)) => {
            confirmed.map_err(|_| ProtocolError::new(44, "API key could not be confirmed"))??;
        }
    }
    let mime = match rendered.format {
        "jpg" => "image/jpeg",
        "png" => "image/png",
        _ => return Err(unavailable()),
    };
    let size = rendered.bytes.len();
    let body = media::buffered_body(
        Bytes::from(rendered.bytes),
        transfer,
        state,
        identity,
        shutdown,
    );
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, size.to_string())
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(body)
        .map_err(|_| ProtocolError::new(0, "artwork response could not be prepared"))
}

#[cfg(test)]
#[path = "artwork_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "artwork_test_support.rs"]
pub(super) mod test_support;
