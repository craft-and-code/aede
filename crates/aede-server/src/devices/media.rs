//! Short-lived, peer-pinned original-file HTTP, without catalog/admin routes.

use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

use super::DeviceTrack;
use crate::subsonic::media::{content_type, requested_range};

const MAX_TRACKS: usize = 64;
const MAX_HEADER: usize = 8192;
const CHUNK_BYTES: usize = 32 * 1024;
const PROGRESS_TIMEOUT: Duration = Duration::from_secs(10);
const PATH_CHECK_INTERVAL: Duration = Duration::from_secs(1);

pub(super) struct Source {
    pub track: DeviceTrack,
    metadata: Metadata,
}

fn validate_path(path: &Path) -> Result<Metadata, String> {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err("device media needs a canonical absolute source path".into());
    }
    let mut prefix = PathBuf::new();
    for part in path.components() {
        prefix.push(part);
        // A Windows drive/verbatim prefix is not a filesystem path until its
        // root has been appended. Inspect that root and every descendant.
        if matches!(part, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&prefix).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("device media source changed to a symbolic link".into());
        }
    }
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.modified().is_err() {
        return Err(
            "device media source must be a regular file with a known modification time".into(),
        );
    }
    Ok(metadata)
}

fn same_source(expected: &Metadata, actual: &Metadata) -> bool {
    let stable = actual.is_file()
        && expected.len() == actual.len()
        && matches!((expected.modified(), actual.modified()), (Ok(left), Ok(right)) if left == right);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        stable && expected.dev() == actual.dev() && expected.ino() == actual.ino()
    }
    #[cfg(not(unix))]
    {
        stable
    }
}

fn open_regular(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(unix_open_flags()?);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: inspect the opened link itself, rather
        // than following a replacement introduced after path validation.
        options.custom_flags(0x0020_0000);
    }
    #[cfg(not(any(unix, windows)))]
    return Err("guarded device media opening is unsupported on this platform".into());
    let file = options.open(path).map_err(|error| error.to_string())?;
    if !file
        .metadata()
        .map_err(|error| error.to_string())?
        .is_file()
    {
        return Err("device media source must be a regular file".into());
    }
    Ok(file)
}

#[cfg(unix)]
fn unix_open_flags() -> Result<i32, String> {
    // O_NONBLOCK prevents a swapped FIFO from retaining a worker forever;
    // O_NOFOLLOW refuses a swapped final symbolic link. These ABI constants
    // are shared by Darwin/BSD but differ among Linux architectures. Unknown
    // Unix targets are refused rather than receiving guessed open flags.
    #[cfg(any(
        target_os = "macos",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    return Ok(0x0004 | 0x0100);
    #[cfg(all(
        target_os = "linux",
        any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "riscv32",
            target_arch = "riscv64",
            target_arch = "loongarch64"
        )
    ))]
    return Ok(0x0800 | 0x0002_0000);
    #[cfg(all(
        target_os = "linux",
        any(
            target_arch = "arm",
            target_arch = "aarch64",
            target_arch = "powerpc",
            target_arch = "powerpc64"
        )
    ))]
    return Ok(0x0800 | 0x8000);
    #[cfg(not(any(
        any(
            target_os = "macos",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "dragonfly"
        ),
        all(
            target_os = "linux",
            any(
                target_arch = "x86",
                target_arch = "x86_64",
                target_arch = "riscv32",
                target_arch = "riscv64",
                target_arch = "loongarch64",
                target_arch = "arm",
                target_arch = "aarch64",
                target_arch = "powerpc",
                target_arch = "powerpc64"
            )
        )
    )))]
    Err("guarded device media opening is unsupported on this Unix target".into())
}

impl Source {
    fn open(&self) -> Result<File, String> {
        if !same_source(&self.metadata, &validate_path(&self.track.path)?) {
            return Err("device media source changed; start a new session".into());
        }
        let file = open_regular(&self.track.path)?;
        self.check(&file)?;
        Ok(file)
    }

    fn check(&self, file: &File) -> Result<(), String> {
        self.check_descriptor(file)?;
        if !same_source(&self.metadata, &validate_path(&self.track.path)?) {
            return Err("device media source changed during transfer".into());
        }
        Ok(())
    }

    fn check_descriptor(&self, file: &File) -> Result<(), String> {
        if !same_source(
            &self.metadata,
            &file.metadata().map_err(|error| error.to_string())?,
        ) {
            return Err("opened device media source changed during transfer".into());
        }
        Ok(())
    }
}

pub(super) fn prepare(paths: Vec<PathBuf>) -> Result<Vec<Source>, String> {
    if paths.is_empty() || paths.len() > MAX_TRACKS {
        return Err("device playback needs between 1 and 64 selection occurrences".into());
    }
    paths
        .into_iter()
        .map(|path| {
            let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
            if path.to_str().is_none() {
                return Err("device media requires UTF-8 source paths".into());
            }
            let metadata = validate_path(&path)?;
            let mut file = open_regular(&path)?;
            if !same_source(
                &metadata,
                &file.metadata().map_err(|error| error.to_string())?,
            ) {
                return Err("device media source changed before inspecting its format".into());
            }
            let tags =
                aede_core::tags::read_from_file(&mut file).map_err(|error| error.to_string())?;
            let mime = content_type(&tags.properties);
            if mime == "application/octet-stream" {
                return Err("this original container has no supported device media type".into());
            }
            if !same_source(
                &metadata,
                &file.metadata().map_err(|error| error.to_string())?,
            ) || !same_source(&metadata, &validate_path(&path)?)
            {
                return Err("device media source changed while inspecting its format".into());
            }
            let title = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Audio")
                .chars()
                .filter(|character| !character.is_control())
                .take(512)
                .collect();
            Ok(Source {
                track: DeviceTrack {
                    path,
                    title,
                    mime,
                    properties: tags.properties,
                },
                metadata,
            })
        })
        .collect()
}

pub(super) struct MediaServer {
    address: SocketAddr,
    urls: Vec<String>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl MediaServer {
    pub async fn start(
        bind: Ipv4Addr,
        peer: Ipv4Addr,
        sources: Vec<Source>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind((bind, 0))
            .await
            .map_err(|error| error.to_string())?;
        let address = listener.local_addr().map_err(|error| error.to_string())?;
        let token = aede_core::accounts::random_token()?;
        let urls = (0..sources.len())
            .map(|index| format!("http://{address}/{token}/{index}"))
            .collect();
        let (shutdown, signal) = watch::channel(false);
        let task = tokio::spawn(run(listener, peer, Arc::new(sources), token, signal));
        Ok(Self {
            address,
            urls,
            shutdown,
            task,
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    pub async fn stop(self) {
        let _ = self.shutdown.send(true);
        let _ = self.task.await;
    }
}

async fn run(
    listener: TcpListener,
    peer: Ipv4Addr,
    sources: Arc<Vec<Source>>,
    token: String,
    mut shutdown: watch::Receiver<bool>,
) {
    let slots = Arc::new(Semaphore::new(4));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => {
                let Ok((stream, address)) = accepted else { break };
                if address.ip() != peer { continue; }
                let Ok(permit) = slots.clone().try_acquire_owned() else { continue };
                let sources = sources.clone();
                let token = token.clone();
                tasks.spawn(async move { let _ = response(stream, sources, &token, permit).await; });
            }
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}

struct MediaRequest {
    index: usize,
    head: bool,
    headers: HeaderMap,
}

fn parse_request(raw: &[u8], token: &str, count: usize) -> Result<MediaRequest, u16> {
    let text = std::str::from_utf8(raw).map_err(|_| 400_u16)?;
    let mut lines = text.strip_suffix("\r\n\r\n").ok_or(400_u16)?.split("\r\n");
    let words = lines.next().ok_or(400_u16)?.split(' ').collect::<Vec<_>>();
    if words.len() != 3 || !matches!(words[2], "HTTP/1.0" | "HTTP/1.1") {
        return Err(400);
    }
    if !matches!(words[0], "GET" | "HEAD") {
        return Err(405);
    }
    let parts = words[1]
        .strip_prefix('/')
        .ok_or(404_u16)?
        .split('/')
        .collect::<Vec<_>>();
    if parts.len() != 2 || parts[0] != token {
        return Err(404);
    }
    let index = parts[1].parse::<usize>().map_err(|_| 404_u16)?;
    if index >= count || index.to_string() != parts[1] {
        return Err(404);
    }
    let mut headers = HeaderMap::new();
    for (index, line) in lines.enumerate() {
        if index >= 32 {
            return Err(400);
        }
        let (name, value) = line.split_once(':').ok_or(400_u16)?;
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| 400_u16)?;
        let value = HeaderValue::from_str(value.trim()).map_err(|_| 400_u16)?;
        if headers.contains_key(&name) {
            return Err(400);
        }
        headers.insert(name, value);
    }
    if headers.contains_key("transfer-encoding")
        || headers
            .get("content-length")
            .is_some_and(|length| length != "0")
        || (words[2] == "HTTP/1.1" && !headers.contains_key("host"))
    {
        return Err(400);
    }
    Ok(MediaRequest {
        index,
        head: words[0] == "HEAD",
        headers,
    })
}

async fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut raw = Vec::with_capacity(1024);
    loop {
        if raw.len() >= MAX_HEADER {
            return Err("device HTTP header is too large".into());
        }
        let mut bytes = [0; 1024];
        let size = (MAX_HEADER - raw.len()).min(bytes.len());
        let count = stream
            .read(&mut bytes[..size])
            .await
            .map_err(|error| error.to_string())?;
        if count == 0 {
            return Err("device HTTP header ended early".into());
        }
        raw.extend_from_slice(&bytes[..count]);
        if let Some(end) = raw.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            if end + 4 != raw.len() {
                return Err("device HTTP requests must have no body or pipeline".into());
            }
            return Ok(raw);
        }
    }
}

async fn write(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), String> {
    tokio::time::timeout(PROGRESS_TIMEOUT, stream.write_all(bytes))
        .await
        .map_err(|_| "device HTTP output stopped making progress".to_string())?
        .map_err(|error| error.to_string())
}

async fn failure(stream: &mut TcpStream, status: u16, extra: &str) -> Result<(), String> {
    write(stream, format!("HTTP/1.1 {status} Refused\r\nContent-Length: 0\r\nConnection: close\r\nCache-Control: no-store\r\n{extra}\r\n").as_bytes()).await
}

struct Cancelled(Arc<AtomicBool>);
impl Drop for Cancelled {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

async fn response(
    mut stream: TcpStream,
    sources: Arc<Vec<Source>>,
    token: &str,
    permit: OwnedSemaphorePermit,
) -> Result<(), String> {
    let raw = match tokio::time::timeout(PROGRESS_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(raw)) => raw,
        _ => return failure(&mut stream, 400, "").await,
    };
    let request = match parse_request(&raw, token, sources.len()) {
        Ok(request) => request,
        Err(status) => return failure(&mut stream, status, "").await,
    };
    let permit = Arc::new(permit);
    let opening_sources = sources.clone();
    let worker_permit = permit.clone();
    let opened = tokio::task::spawn_blocking(move || {
        let _permit = worker_permit;
        opening_sources[request.index].open()
    });
    let file = match tokio::time::timeout(PROGRESS_TIMEOUT, opened).await {
        Ok(Ok(Ok(file))) => file,
        _ => return failure(&mut stream, 409, "").await,
    };
    let source = &sources[request.index];
    let range_headers = if request.head {
        HeaderMap::new()
    } else {
        request.headers
    };
    let range = match requested_range(&range_headers, source.metadata.len()) {
        Ok(Some(range)) => range,
        Ok(None) => {
            return failure(
                &mut stream,
                416,
                &format!("Content-Range: bytes */{}\r\n", source.metadata.len()),
            )
            .await;
        }
        Err(_) => return failure(&mut stream, 400, "").await,
    };
    let mut header = format!(
        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n",
        if range.partial { 206 } else { 200 },
        source.track.mime,
        range.length
    );
    if range.partial {
        header.push_str(&format!(
            "Content-Range: bytes {}-{}/{}\r\n",
            range.start,
            range.start + range.length - 1,
            source.metadata.len()
        ));
    }
    header.push_str("\r\n");
    write(&mut stream, header.as_bytes()).await?;
    if request.head {
        return Ok(());
    }
    let cancelled = Cancelled(Arc::new(AtomicBool::new(false)));
    let worker_cancelled = cancelled.0.clone();
    let (sender, mut chunks) = mpsc::channel(2);
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let source = &sources[request.index];
        let mut file = file;
        let result = (|| -> Result<(), String> {
            file.seek(SeekFrom::Start(range.start))
                .map_err(|error| error.to_string())?;
            let mut remaining = range.length;
            let mut path_checked = Instant::now();
            while remaining > 0 && !worker_cancelled.load(Ordering::Acquire) {
                let mut bytes = vec![0; remaining.min(CHUNK_BYTES as u64) as usize];
                file.read_exact(&mut bytes)
                    .map_err(|error| error.to_string())?;
                remaining -= bytes.len() as u64;
                source.check_descriptor(&file)?;
                if remaining == 0 || path_checked.elapsed() >= PATH_CHECK_INTERVAL {
                    source.check(&file)?;
                    path_checked = Instant::now();
                }
                if sender.blocking_send(Ok(bytes)).is_err() {
                    return Ok(());
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = sender.blocking_send(Err(error));
        }
    });
    while let Some(chunk) = tokio::time::timeout(PROGRESS_TIMEOUT, chunks.recv())
        .await
        .map_err(|_| "device media read stopped making progress")?
    {
        write(&mut stream, &chunk?).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "media_tests.rs"]
mod tests;
