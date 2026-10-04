//! Bounded original-file transport, without claiming that a transfer was played.
//! Client-declared scrobbles use the separate personal-store contract.

use std::fs::{self, File, Metadata};
use std::io::{self, Read, Seek, SeekFrom};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use aede_core::clock;
use aede_core::tags::AudioProperties;
use aede_core::user::EntityRef;
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use hyper::body::{Body as HttpBody, Bytes, Frame, SizeHint};
use tokio::sync::{OwnedSemaphorePermit, broadcast, mpsc};

use super::ProtocolError;
use super::authentication::{self, Identity};
use crate::ApiState;
use crate::playback_api::{TrackSource, source_from_catalog, validate_source};

const CHUNK_BYTES: usize = 32 * 1024;
const QUEUED_CHUNKS: usize = 2;
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_INTERVAL: Duration = Duration::from_secs(1);
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
const SOURCE_PROGRESS_TIMEOUT: Duration = Duration::from_secs(10);

/// The MIME type of original audio, independent of its private filesystem path.
pub(crate) fn content_type(properties: &AudioProperties) -> &'static str {
    match suffix(properties) {
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "aiff" => "audio/aiff",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wv" => "audio/wavpack",
        _ => "application/octet-stream",
    }
}

/// A conservative original-container suffix for the public song projection.
pub(super) fn suffix(properties: &AudioProperties) -> &'static str {
    let container = if properties.container.is_empty() {
        properties.codec.as_str()
    } else {
        properties.container.as_str()
    };
    match container {
        "flac" => "flac",
        "mp3" => "mp3",
        "ogg" => "ogg",
        "opus" => "opus",
        "wav" | "wave" => "wav",
        "aiff" | "aif" => "aiff",
        "mp4" | "m4a" | "m4b" => "m4a",
        "aac" | "adts" => "aac",
        "wavpack" | "wv" => "wv",
        "ape" => "ape",
        "dsf" => "dsf",
        "dff" => "dff",
        _ => "bin",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ByteRange {
    pub(crate) start: u64,
    pub(crate) length: u64,
    pub(crate) partial: bool,
}

pub(crate) fn requested_range(
    headers: &HeaderMap,
    size: u64,
) -> Result<Option<ByteRange>, ProtocolError> {
    let mut ranges = headers.get_all(header::RANGE).iter();
    let range = ranges.next();
    if ranges.next().is_some() {
        return Err(ProtocolError::new(10, "Only one Range header is supported"));
    }
    let mut conditions = headers.get_all(header::IF_RANGE).iter();
    let conditional = conditions.next().is_some();
    if conditions.next().is_some() {
        return Err(ProtocolError::new(
            10,
            "Only one If-Range header is supported",
        ));
    }
    // No strong entity validator is published. An unrecognized If-Range
    // cannot authorize appending bytes to a previously downloaded entity.
    if conditional || range.is_none() {
        return Ok(Some(ByteRange {
            start: 0,
            length: size,
            partial: false,
        }));
    }
    let value = range.and_then(|range| range.to_str().ok()).unwrap_or("");
    Ok(parse_range(value, size))
}

fn parse_range(value: &str, size: u64) -> Option<ByteRange> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
    if size == 0 || value.contains(',') {
        return None;
    }
    let decimal = |value: &str| {
        (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten()
    };
    let (start, end) = if start.is_empty() {
        let suffix = decimal(end)?.min(size);
        if suffix == 0 {
            return None;
        }
        (size - suffix, size - 1)
    } else {
        let start = decimal(start)?;
        if start >= size {
            return None;
        }
        let end = if end.is_empty() {
            size - 1
        } else {
            decimal(end)?.min(size - 1)
        };
        if end < start {
            return None;
        }
        (start, end)
    };
    Some(ByteRange {
        start,
        length: end.checked_sub(start)?.checked_add(1)?,
        partial: true,
    })
}

fn unavailable() -> ProtocolError {
    ProtocolError::new(
        70,
        "Media source is unavailable or changed; refresh the catalog",
    )
}

fn valid_opened(source: &TrackSource, metadata: &Metadata) -> bool {
    metadata.is_file()
        && metadata.len() == source.file.size
        && clock::mtime_seconds(metadata) == source.file.mtime
        && clock::mtime_subseconds(metadata) == source.mtime_subseconds
}

pub(super) fn same_file(left: &Metadata, right: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        left.dev() == right.dev() && left.ino() == right.ino()
    }
    #[cfg(not(unix))]
    {
        left.len() == right.len()
            && matches!((left.modified(), right.modified()), (Ok(left), Ok(right)) if left == right)
    }
}

struct Opened {
    file: File,
    source: TrackSource,
    _permit: Arc<OwnedSemaphorePermit>,
}

fn open_source(source: TrackSource, permit: OwnedSemaphorePermit) -> Result<Opened, ProtocolError> {
    validate_source(&source).map_err(|_| unavailable())?;
    let before = fs::symlink_metadata(&source.path).map_err(|_| unavailable())?;
    let file = File::open(&source.path).map_err(|_| unavailable())?;
    let metadata = file.metadata().map_err(|_| unavailable())?;
    if !valid_opened(&source, &metadata) || !same_file(&before, &metadata) {
        return Err(unavailable());
    }
    validate_source(&source).map_err(|_| unavailable())?;
    Ok(Opened {
        file,
        source,
        _permit: Arc::new(permit),
    })
}

/// Retrieve the exact catalogued original, without DSP or listening mutations.
pub(super) async fn response(
    state: ApiState,
    identity: Identity,
    track: EntityRef,
    headers: HeaderMap,
    download: bool,
) -> Result<Response, ProtocolError> {
    authentication::require_audio(&identity)?;
    let source = {
        let catalog = state.catalog.read().await;
        let catalog = catalog.as_ref().ok_or_else(unavailable)?;
        source_from_catalog(catalog, &track).map_err(|_| unavailable())?
    };
    let range = requested_range(&headers, source.file.size)?;
    let permit = state
        .playback_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ProtocolError::new(0, "Too many active audio transfers; retry later"))?;
    let mut shutdown = state.shutdown.subscribe();
    let opening = tokio::time::timeout(
        OPEN_TIMEOUT,
        tokio::task::spawn_blocking(move || open_source(source, permit)),
    );
    let opened = tokio::select! {
        _ = shutdown.recv() => return Err(ProtocolError::new(0, "server is shutting down")),
        opened = opening => opened.map_err(|_| unavailable())?.map_err(|_| unavailable())??,
    };
    tokio::select! {
        _ = shutdown.recv() => return Err(ProtocolError::new(0, "server is shutting down")),
        confirmation = tokio::time::timeout(AUTH_TIMEOUT, authentication::recheck(&state, &identity)) => {
            confirmation.map_err(|_| ProtocolError::new(40, "API key could not be confirmed"))??;
        }
    }
    let mut response = Response::builder()
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(
            header::CONTENT_TYPE,
            content_type(&opened.source.file.properties),
        )
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    let Some(range) = range else {
        return response
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(
                header::CONTENT_RANGE,
                format!("bytes */{}", opened.source.file.size),
            )
            .header(header::CONTENT_LENGTH, "0")
            .body(Body::empty())
            .map_err(|_| ProtocolError::new(0, "Media response could not be prepared"));
    };
    if range.partial {
        response = response.status(StatusCode::PARTIAL_CONTENT).header(
            header::CONTENT_RANGE,
            format!(
                "bytes {}-{}/{}",
                range.start,
                range.start + range.length - 1,
                opened.source.file.size
            ),
        );
    }
    if download {
        response = response.header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"audio.{}\"",
                suffix(&opened.source.file.properties)
            ),
        );
    }
    let body = original_body(opened, range, state, identity, shutdown);
    response
        .header(header::CONTENT_LENGTH, range.length.to_string())
        .body(Body::new(body))
        .map_err(|_| ProtocolError::new(0, "Media response could not be prepared"))
}

fn read_original(
    opened: &mut Opened,
    range: ByteRange,
    sender: mpsc::Sender<io::Result<Bytes>>,
    cancelled: Arc<AtomicBool>,
) -> io::Result<()> {
    // The permit belongs to the real filesystem worker, not the HTTP waiter.
    // Dropping a timed-out response therefore cannot admit unbounded reads.
    opened.file.seek(SeekFrom::Start(range.start))?;
    let mut remaining = range.length;
    while remaining > 0 {
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        let count = usize::try_from(remaining.min(CHUNK_BYTES as u64))
            .map_err(|_| io::Error::other("invalid media chunk size"))?;
        let mut bytes = vec![0; count];
        opened.file.read_exact(&mut bytes)?;
        remaining -= count as u64;
        if remaining == 0 {
            let metadata = opened.file.metadata()?;
            if !valid_opened(&opened.source, &metadata)
                || validate_source(&opened.source).is_err()
                || !same_file(&fs::symlink_metadata(&opened.source.path)?, &metadata)
            {
                return Err(io::Error::other("media source changed during transfer"));
            }
        }
        if sender.blocking_send(Ok(Bytes::from(bytes))).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

type MediaChunk = io::Result<Bytes>;

fn original_body(
    opened: Opened,
    range: ByteRange,
    state: ApiState,
    identity: Identity,
    shutdown: broadcast::Receiver<()>,
) -> OriginalBody {
    let cancelled = Arc::new(AtomicBool::new(false));
    let permit = opened._permit.clone();
    let (producer, chunks) = mpsc::channel(QUEUED_CHUNKS);
    let worker_cancelled = cancelled.clone();
    tokio::task::spawn_blocking(move || {
        let mut opened = opened;
        if let Err(failure) = read_original(&mut opened, range, producer.clone(), worker_cancelled)
        {
            let _ = producer.blocking_send(Err(failure));
        }
    });
    guarded_body(
        chunks,
        permit,
        range.length,
        cancelled,
        state,
        identity,
        shutdown,
    )
}

/// Supervise an already validated in-memory original or derived artwork.
/// Its transfer slot remains held through consumer completion or cancellation.
pub(super) fn buffered_body(
    mut bytes: Bytes,
    permit: Arc<OwnedSemaphorePermit>,
    state: ApiState,
    identity: Identity,
    shutdown: broadcast::Receiver<()>,
) -> Body {
    let length = bytes.len() as u64;
    let cancelled = Arc::new(AtomicBool::new(false));
    let (producer, chunks) = mpsc::channel(QUEUED_CHUNKS);
    let worker_cancelled = cancelled.clone();
    let worker_permit = permit.clone();
    tokio::spawn(async move {
        let _permit = worker_permit;
        while !bytes.is_empty() && !worker_cancelled.load(Ordering::Acquire) {
            let chunk = bytes.split_to(bytes.len().min(CHUNK_BYTES));
            if producer.send(Ok(chunk)).await.is_err() {
                break;
            }
        }
    });
    Body::new(guarded_body(
        chunks, permit, length, cancelled, state, identity, shutdown,
    ))
}

fn guarded_body(
    chunks: mpsc::Receiver<MediaChunk>,
    permit: Arc<OwnedSemaphorePermit>,
    length: u64,
    cancelled: Arc<AtomicBool>,
    state: ApiState,
    identity: Identity,
    shutdown: broadcast::Receiver<()>,
) -> OriginalBody {
    let (output, receiver) = mpsc::channel(QUEUED_CHUNKS);
    tokio::spawn(forward_original(
        chunks,
        output,
        cancelled.clone(),
        state,
        identity,
        shutdown,
    ));
    OriginalBody {
        receiver,
        cancelled,
        remaining: length,
        ended: false,
        _permit: permit,
    }
}

async fn forward_original(
    mut chunks: mpsc::Receiver<MediaChunk>,
    output: mpsc::Sender<MediaChunk>,
    cancelled: Arc<AtomicBool>,
    state: ApiState,
    identity: Identity,
    mut shutdown: broadcast::Receiver<()>,
) {
    let mut authorization =
        tokio::time::interval_at(tokio::time::Instant::now() + AUTH_INTERVAL, AUTH_INTERVAL);
    authorization.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pending = None;
    let mut source_finished = false;
    let mut source_progress = tokio::time::Instant::now();
    loop {
        let source_timeout = tokio::time::sleep_until(source_progress + SOURCE_PROGRESS_TIMEOUT);
        tokio::pin!(source_timeout);
        tokio::select! {
            _ = shutdown.recv() => break,
            _ = output.closed() => break,
            _ = &mut source_timeout, if pending.is_none() && !source_finished => break,
            _ = authorization.tick() => {
                let confirmation = tokio::select! {
                    _ = shutdown.recv() => break,
                    _ = output.closed() => break,
                    confirmation = tokio::time::timeout(AUTH_TIMEOUT, authentication::recheck(&state, &identity)) => confirmation,
                };
                if !matches!(confirmation, Ok(Ok(()))) {
                    break;
                }
            }
            chunk = chunks.recv(), if pending.is_none() && !source_finished => match chunk {
                Some(Ok(chunk)) => {
                    source_progress = tokio::time::Instant::now();
                    pending = Some(chunk);
                }
                Some(Err(_)) => break,
                // Authorization must outlive the reader: small originals may
                // already be buffered while the HTTP consumer is still idle.
                None => source_finished = true,
            },
            // Reserving is cancellation-safe. Keeping the bytes outside this
            // future prevents an authorization tick from losing file data.
            reservation = output.reserve(), if pending.is_some() => match reservation {
                Ok(slot) => {
                    if let Some(chunk) = pending.take() {
                        slot.send(Ok(chunk));
                        source_progress = tokio::time::Instant::now();
                    }
                }
                Err(_) => break,
            },
        }
    }
    cancelled.store(true, Ordering::Release);
}

struct OriginalBody {
    receiver: mpsc::Receiver<MediaChunk>,
    cancelled: Arc<AtomicBool>,
    remaining: u64,
    ended: bool,
    _permit: Arc<OwnedSemaphorePermit>,
}

impl Drop for OriginalBody {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl HttpBody for OriginalBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<io::Result<Frame<Bytes>>>> {
        let body = self.get_mut();
        if body.ended || body.remaining == 0 {
            body.ended = true;
            return Poll::Ready(None);
        }
        if body.cancelled.load(Ordering::Acquire) {
            body.ended = true;
            return Poll::Ready(Some(Err(io::Error::other("media transfer stopped"))));
        }
        match body.receiver.poll_recv(context) {
            Poll::Ready(Some(Ok(bytes))) => {
                let count = bytes.len() as u64;
                if count > body.remaining {
                    body.ended = true;
                    return Poll::Ready(Some(Err(io::Error::other(
                        "media transfer exceeded its range",
                    ))));
                }
                body.remaining -= count;
                Poll::Ready(Some(Ok(Frame::data(bytes))))
            }
            Poll::Ready(Some(Err(failure))) => {
                body.ended = true;
                Poll::Ready(Some(Err(failure)))
            }
            Poll::Ready(None) => {
                body.ended = true;
                if body.remaining == 0 {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(io::Error::other("media transfer ended early"))))
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.ended || self.remaining == 0
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.remaining)
    }
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
