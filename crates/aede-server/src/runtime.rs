//! Server startup, catalog reloads and graceful shutdown.

use super::*;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::service::TowerToHyperService;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::task::JoinSet;
use tower::ServiceExt as _;

const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP_HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const TLS_CONNECTION_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECTION_DRAIN_TIMEOUT: Duration = Duration::from_secs(15);
const HTTP_WRITE_PROGRESS_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP_MAX_HEADERS: usize = 64;
const HTTP_MAX_BUFFER: usize = 32 * 1024;
const RUNTIME_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Keeps a TCP admission permit alive for every consumer of the stream.
///
/// `hyper` transfers its I/O object into an upgraded WebSocket. Holding the
/// permit beside the object instead of beside the HTTP connection task keeps
/// the connection limit accurate after that task returns.
struct ConnectionStream<S> {
    stream: S,
    _permit: tokio::sync::OwnedSemaphorePermit,
    write_deadline: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl<S> ConnectionStream<S> {
    fn new(stream: S, permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        Self {
            stream,
            _permit: permit,
            write_deadline: None,
        }
    }

    fn clear_write_deadline(&mut self) {
        self.write_deadline = None;
    }

    fn pending_write(&mut self, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let deadline = self
            .write_deadline
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(HTTP_WRITE_PROGRESS_TIMEOUT)));
        if std::future::Future::poll(deadline.as_mut(), context).is_ready() {
            self.clear_write_deadline();
            Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "HTTP response made no write progress before its deadline",
            )))
        } else {
            Poll::Pending
        }
    }

    fn write_result(
        &mut self,
        outcome: Poll<std::io::Result<usize>>,
        context: &mut Context<'_>,
    ) -> Poll<std::io::Result<usize>> {
        match outcome {
            Poll::Ready(Ok(written)) => {
                if written > 0 {
                    self.clear_write_deadline();
                }
                Poll::Ready(Ok(written))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => self.pending_write(context).map(|result| result.map(|()| 0)),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for ConnectionStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().stream).poll_read(context, buffer)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for ConnectionStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let stream = self.as_mut().get_mut();
        let outcome = Pin::new(&mut stream.stream).poll_write(context, buffer);
        stream.write_result(outcome, context)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let stream = self.get_mut();
        let outcome = Pin::new(&mut stream.stream).poll_write_vectored(context, buffers);
        stream.write_result(outcome, context)
    }

    fn is_write_vectored(&self) -> bool {
        self.stream.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let stream = self.get_mut();
        match Pin::new(&mut stream.stream).poll_flush(context) {
            Poll::Ready(Ok(())) => {
                stream.clear_write_deadline();
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => stream.pending_write(context),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let stream = self.get_mut();
        match Pin::new(&mut stream.stream).poll_shutdown(context) {
            Poll::Ready(Ok(())) => {
                stream.clear_write_deadline();
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => stream.pending_write(context),
        }
    }
}

pub(super) fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

pub(super) enum Reload {
    Busy,
    Unchanged,
    Loaded(
        Option<(SystemTime, u64)>,
        Box<Result<Option<Catalog>, store::StoreError>>,
    ),
    LockFailed(std::io::Error),
}

pub(super) fn report_reload_failure(
    path: &Path,
    reported_failure: &mut Option<Option<(SystemTime, u64)>>,
    events: &broadcast::Sender<CatalogEvent>,
    message: impl std::fmt::Display,
) {
    let current = stamp(path);
    if *reported_failure != Some(current) {
        let message = message.to_string();
        eprintln!("API catalog reload failed: {message}");
        let _ = events.send(CatalogEvent::Error {
            operation: "catalog_reload",
            code: "catalog_reload_failed",
            message,
        });
        *reported_failure = Some(current);
    }
}

pub(super) async fn refresh_catalog(
    path: &Path,
    state: &ApiState,
    known: &mut Option<(SystemTime, u64)>,
    reported_failure: &mut Option<Option<(SystemTime, u64)>>,
) {
    let _gate = state.reload_gate.lock().await;
    if stamp(path) == *known {
        return;
    }
    let loaded_stamp = *state.loaded_stamp.read().await;
    if stamp(path) == loaded_stamp {
        *known = loaded_stamp;
        return;
    }
    let to_load = path.to_path_buf();
    let previous = *known;
    let loaded = tokio::task::spawn_blocking(move || {
        let data_dir = to_load.parent().unwrap_or(Path::new("."));
        let _guard = match StoreLock::try_acquire(data_dir) {
            Ok(lock) => lock,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Reload::Busy,
            Err(error) => return Reload::LockFailed(error),
        };
        let current = stamp(&to_load);
        if current == previous {
            return Reload::Unchanged;
        }
        Reload::Loaded(current, Box::new(store::load(&to_load)))
    })
    .await;
    match loaded {
        Ok(Reload::Loaded(current, outcome)) => match *outcome {
            Ok(catalog) => {
                let scanned_at = catalog.as_ref().map(|c| c.scanned_at);
                *state.catalog.write().await = catalog;
                *state.loaded_stamp.write().await = current;
                *known = current;
                *reported_failure = None;
                let _ = state
                    .events
                    .send(CatalogEvent::CatalogChanged { scanned_at });
            }
            Err(error) => report_reload_failure(path, reported_failure, &state.events, error),
        },
        Ok(Reload::Busy | Reload::Unchanged) => {}
        Ok(Reload::LockFailed(error)) => {
            report_reload_failure(
                path,
                reported_failure,
                &state.events,
                format!("lock: {error}"),
            );
        }
        Err(error) => {
            report_reload_failure(
                path,
                reported_failure,
                &state.events,
                format!("task: {error}"),
            );
        }
    }
}

pub(super) fn watch_catalog(path: PathBuf, state: ApiState) -> impl Future<Output = ()> {
    let shutdown = state.shutdown.subscribe();
    watch_catalog_until(path, state, shutdown)
}

async fn watch_catalog_until(
    path: PathBuf,
    state: ApiState,
    mut shutdown: broadcast::Receiver<()>,
) {
    let mut known = *state.loaded_stamp.read().await;
    let mut reported_failure: Option<Option<(SystemTime, u64)>> = None;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            _ = interval.tick() => {
                refresh_catalog(&path, &state, &mut known, &mut reported_failure).await;
            }
        }
    }
}

pub(super) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let termination = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
        match termination {
            Ok(mut termination) => tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    if let Err(error) = result {
                        eprintln!("API shutdown signal failed: {error}");
                    }
                }
                _ = termination.recv() => {}
            },
            Err(error) => {
                eprintln!("API termination signal setup failed: {error}");
                if let Err(error) = tokio::signal::ctrl_c().await {
                    eprintln!("API shutdown signal failed: {error}");
                }
            }
        }
    }
    #[cfg(not(unix))]
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("API shutdown signal failed: {error}");
    }
}

pub(super) async fn run_http(
    listener: tokio::net::TcpListener,
    path: PathBuf,
    state: ApiState,
    #[cfg(unix)] command_listener: tokio::net::UnixListener,
    #[cfg(unix)] command_socket: PathBuf,
    #[cfg(unix)] command_validator: Arc<CommandValidator>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), std::io::Error> {
    let watcher = tokio::spawn(watch_catalog(path, state.clone()));
    #[cfg(unix)]
    let commands = tokio::spawn(delegation::accept_commands(
        command_listener,
        state.clone(),
        command_validator,
    ));
    let cleanup_shutdown = state.shutdown.clone();
    let address = listener.local_addr()?;
    // Subscribe before spawning the trigger; an immediately ready shutdown
    // must not disappear before the listener future receives its first poll.
    let shutdown_receiver = state.shutdown.subscribe();
    let sender = state.shutdown.clone();
    let shutdown_task = tokio::spawn(async move {
        shutdown.await;
        let _ = sender.send(());
    });
    let result = serve_connections(
        listener,
        None,
        router(state.clone(), address),
        &state,
        shutdown_receiver,
    )
    .await;
    shutdown_task.abort();
    // Also stop the command listener if serving fails before a signal arrives.
    let _ = cleanup_shutdown.send(());
    watcher.abort();
    #[cfg(unix)]
    {
        if let Err(error) = commands.await {
            eprintln!("Aède command shutdown failed: {error}");
        }
        let _ = std::fs::remove_file(&command_socket);
        if let Some(parent) = command_socket.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    jobs::wait_for_jobs(&state).await;
    result
}

async fn run_tls(
    listener: tokio::net::TcpListener,
    tls: TlsServer,
    path: PathBuf,
    state: ApiState,
    #[cfg(unix)] command_listener: tokio::net::UnixListener,
    #[cfg(unix)] command_socket: PathBuf,
    #[cfg(unix)] command_validator: Arc<CommandValidator>,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), std::io::Error> {
    let watcher = tokio::spawn(watch_catalog(path, state.clone()));
    #[cfg(unix)]
    let commands = tokio::spawn(delegation::accept_commands(
        command_listener,
        state.clone(),
        command_validator,
    ));
    let TlsServer {
        acceptor,
        authority,
    } = tls;
    let serving = serve_tls(
        listener,
        acceptor,
        remote_router(state.clone(), authority),
        state.clone(),
    );
    let sender = state.shutdown.clone();
    let shutdown_task = tokio::spawn(async move {
        shutdown.await;
        let _ = sender.send(());
    });
    let result = serving.await;
    shutdown_task.abort();
    // Also stop the watcher and command listener if serving fails before a
    // termination signal arrives.
    let _ = state.shutdown.send(());
    watcher.abort();
    #[cfg(unix)]
    {
        if let Err(error) = commands.await {
            eprintln!("Aède command shutdown failed: {error}");
        }
        let _ = std::fs::remove_file(&command_socket);
        if let Some(parent) = command_socket.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
    jobs::wait_for_jobs(&state).await;
    result
}

/// Serve bounded HTTPS connections until the shared shutdown channel closes.
///
/// The function is separate from startup so tests can exercise real TLS and
/// WebSocket upgrades without requiring a process signal or a data lock.
pub(super) fn serve_tls(
    listener: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    application: Router,
    state: ApiState,
) -> impl Future<Output = Result<(), std::io::Error>> + Send + 'static {
    let shutdown = state.shutdown.subscribe();
    async move { serve_connections(listener, Some(acceptor), application, &state, shutdown).await }
}

/// Apply the same admission, header and stalled-write bounds to both listeners.
///
/// Local HTTP still waits for active requests at shutdown: a bodyless scan is
/// synchronous and may legitimately take longer than the HTTPS drain budget.
/// Incomplete headers and blocked writes have their own finite deadlines.
async fn serve_connections(
    listener: tokio::net::TcpListener,
    acceptor: Option<tokio_rustls::TlsAcceptor>,
    application: Router,
    state: &ApiState,
    mut shutdown: broadcast::Receiver<()>,
) -> Result<(), std::io::Error> {
    // Capture this before accepting. A WebSocket upgrade leaves the HTTP task,
    // but retains its permit in `ConnectionStream`; shutdown below waits until
    // every permit this listener could have issued has returned.
    let connection_capacity = state.connection_slots.available_permits();
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            _ = shutdown.recv() => break,
            Some(_) = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    let permit = match state.connection_slots.clone().try_acquire_owned() {
                        Ok(permit) => permit,
                        Err(_) => continue,
                    };
                    let connection_shutdown = state.shutdown.subscribe();
                    let connection_acceptor = acceptor.clone();
                    let connection_application = application.clone();
                    connections.spawn(async move {
                        serve_connection(
                            stream,
                            permit,
                            connection_acceptor,
                            connection_application,
                            connection_shutdown,
                        )
                        .await;
                    });
                }
                Err(error) if is_connection_error(&error) => {},
                Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            },
        }
    }
    drop(listener);
    if acceptor.is_some() {
        drain_tls_connections(
            &mut connections,
            state.connection_slots.as_ref(),
            connection_capacity,
        )
        .await;
    } else {
        while connections.join_next().await.is_some() {}
        drain_upgraded_connections(
            state.connection_slots.as_ref(),
            connection_capacity,
            tokio::time::Instant::now() + CONNECTION_DRAIN_TIMEOUT,
        )
        .await;
    }
    Ok(())
}

async fn serve_connection(
    stream: tokio::net::TcpStream,
    permit: tokio::sync::OwnedSemaphorePermit,
    acceptor: Option<tokio_rustls::TlsAcceptor>,
    application: Router,
    mut shutdown: broadcast::Receiver<()>,
) {
    if let Some(acceptor) = acceptor {
        let stream = tokio::select! {
            accepted = tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)) => match accepted {
                Ok(Ok(stream)) => stream,
                Ok(Err(_)) | Err(_) => return,
            },
            _ = shutdown.recv() => return,
        };
        serve_http_connection(
            ConnectionStream::new(stream, permit),
            application,
            shutdown,
            true,
        )
        .await;
    } else {
        serve_http_connection(
            ConnectionStream::new(stream, permit),
            application,
            shutdown,
            false,
        )
        .await;
    }
}

async fn serve_http_connection<S>(
    stream: ConnectionStream<S>,
    application: Router,
    mut shutdown: broadcast::Receiver<()>,
    remote: bool,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let service =
        application.map_request(|request: Request<Incoming>| request.map(axum::body::Body::new));
    let service = TowerToHyperService::new(service);
    let mut http = http1::Builder::new();
    http.timer(TokioTimer::new())
        .header_read_timeout(HTTP_HEADER_TIMEOUT)
        .max_headers(HTTP_MAX_HEADERS)
        .max_buf_size(HTTP_MAX_BUFFER)
        // TCP preserves its efficient vectored writes. TLS does not provide
        // that capability, so avoid its queue/copy choice heuristic.
        .writev(!remote);
    let connection = http
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades();
    tokio::pin!(connection);
    tokio::select! {
        _ = &mut connection => {},
        _ = shutdown.recv() => {
            connection.as_mut().graceful_shutdown();
            if remote {
                let _ = tokio::time::timeout(TLS_CONNECTION_DRAIN_TIMEOUT, &mut connection).await;
            } else {
                let _ = connection.await;
            }
        }
    }
}

async fn drain_tls_connections(
    connections: &mut JoinSet<()>,
    connection_slots: &Semaphore,
    connection_capacity: usize,
) {
    let deadline = tokio::time::Instant::now() + CONNECTION_DRAIN_TIMEOUT;
    while !connections.is_empty() {
        if tokio::time::timeout_at(deadline, connections.join_next())
            .await
            .is_err()
        {
            connections.abort_all();
            while connections.join_next().await.is_some() {}
            break;
        }
    }
    drain_upgraded_connections(connection_slots, connection_capacity, deadline).await;
}

async fn drain_upgraded_connections(
    connection_slots: &Semaphore,
    connection_capacity: usize,
    deadline: tokio::time::Instant,
) {
    // Upgraded WebSocket callbacks outlive Hyper's connection future. Their
    // stream still owns a permit, so wait for the shared capacity rather than
    // considering only the now-empty JoinSet.
    while connection_slots.available_permits() < connection_capacity {
        if tokio::time::timeout_at(deadline, tokio::time::sleep(Duration::from_millis(10)))
            .await
            .is_err()
        {
            break;
        }
    }
}

fn is_connection_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

/// Stop the runtime without allowing an uninterruptible filesystem operation
/// in a blocking worker to hold process shutdown forever.
///
/// Accepted jobs and command requests have already finished before this runs.
/// Timed-out workers are not forcibly interrupted: playback decoding only
/// reads source audio, and listening-history saves use the core's atomic file
/// writer rather than risking a partial user-data file.
fn shutdown_runtime(runtime: tokio::runtime::Runtime, timeout: Duration) {
    runtime.shutdown_timeout(timeout);
}

/// Starts the backwards-compatible loopback HTTP API until the process stops.
///
/// Calls `on_ready` with the bound address, including the assigned port when
/// `port` is zero. The caller decides how to present that address.
pub fn serve(
    data_dir: &Path,
    port: u16,
    admin_token: Option<String>,
    on_scan: impl Fn(&mut (dyn FnMut(Progress) + Send)) -> Result<(), String> + Send + Sync + 'static,
    on_command: impl Fn(&[String], &str) -> bool + Send + Sync + 'static,
    on_job: impl Fn(JobRequest, Arc<std::sync::atomic::AtomicBool>) -> Result<JobOutput, String>
    + Send
    + Sync
    + 'static,
    on_ready: impl FnOnce(SocketAddr),
) -> Result<(), Box<dyn Error>> {
    serve_with_options(
        data_dir,
        ServerOptions::loopback(port),
        admin_token,
        on_scan,
        on_command,
        on_job,
        on_ready,
    )
}

/// Starts the local HTTP API or the explicit authenticated HTTPS listener.
///
/// A `tls: None` option preserves the loopback-only HTTP contract. A TLS
/// option requires an initialized account store before any public socket is
/// announced; legacy administrative-token and administrative routes remain
/// local-only. Shutdown still waits for accepted installation jobs and local
/// command requests; after that it waits at most five seconds for background
/// playback workers that cannot be safely interrupted.
pub fn serve_with_options(
    data_dir: &Path,
    options: ServerOptions,
    admin_token: Option<String>,
    on_scan: impl Fn(&mut (dyn FnMut(Progress) + Send)) -> Result<(), String> + Send + Sync + 'static,
    on_command: impl Fn(&[String], &str) -> bool + Send + Sync + 'static,
    on_job: impl Fn(JobRequest, Arc<std::sync::atomic::AtomicBool>) -> Result<JobOutput, String>
    + Send
    + Sync
    + 'static,
    on_ready: impl FnOnce(SocketAddr),
) -> Result<(), Box<dyn Error>> {
    #[cfg(not(unix))]
    let _ = on_command;
    #[cfg(unix)]
    let _server_lock = delegation::ServerLock::acquire(data_dir)?;
    // Read and validate certificate material before binding a public socket.
    let tls = configure(&options)?;
    let remote = tls.is_some();
    let path = store::catalog_path(data_dir);
    let (catalog, loaded_stamp) = {
        let _guard = StoreLock::acquire(data_dir)?;
        (store::load(&path)?, stamp(&path))
    };
    let catalog = catalog.ok_or_else(|| {
        format!(
            "no catalog in {}. Run aede scan <folder> first",
            path.display()
        )
    })?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async move {
        let (events, _) = broadcast::channel(32);
        let (shutdown, _) = broadcast::channel(1);
        let admin = (!remote).then(|| Admin {
            token: admin_token.unwrap_or_default(),
            data_dir: data_dir.to_path_buf(),
            scan: Arc::new(on_scan),
            job: Arc::new(on_job),
        });
        let state = ApiState {
            data_dir: data_dir.to_path_buf(),
            catalog: Arc::new(RwLock::new(Some(catalog))),
            loaded_stamp: Arc::new(RwLock::new(loaded_stamp)),
            reload_gate: Arc::new(Mutex::new(())),
            events,
            shutdown,
            admin,
            auth: Arc::new(auth::AuthState::default()),
            subsonic: Arc::new(subsonic::authentication::Authentication::default()),
            next_task_id: Arc::new(AtomicU64::new(1)),
            scan_activity: state::ScanActivity::default(),
            connection_slots: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            remote_request_slots: Arc::new(Semaphore::new(MAX_REMOTE_REQUESTS)),
            websocket_slots: Arc::new(Semaphore::new(MAX_WEBSOCKETS)),
            playback_slots: Arc::new(Semaphore::new(MAX_PLAYBACKS)),
            player_profiles: Arc::new(std::sync::Mutex::new(std::collections::BTreeSet::new())),
            inspection_slots: Arc::new(Semaphore::new(2)),
            jobs: Arc::new(jobs::JobRegistry::default()),
            #[cfg(unix)]
            tasks: Arc::new(delegation::TaskRegistry::default()),
        };
        // Refuse malformed credentials before announcing a ready listener.
        let accounts = auth::load_accounts(&state).map_err(|failure| failure.message)?;
        if remote && accounts.is_none() {
            return Err("an HTTPS listener requires initialized accounts".into());
        }
        // Do not publish a socket until account-mode validation succeeds.
        let listener = tokio::net::TcpListener::bind(options.bind).await?;
        #[cfg(unix)]
        let (command_listener, command_socket) = delegation::bind_command_socket(data_dir)?;
        let address = listener.local_addr()?;
        on_ready(address);
        match tls {
            Some(tls) => {
                run_tls(
                    listener,
                    tls,
                    path,
                    state,
                    #[cfg(unix)]
                    command_listener,
                    #[cfg(unix)]
                    command_socket,
                    #[cfg(unix)]
                    Arc::new(on_command),
                    shutdown_signal(),
                )
                .await?;
            }
            None => {
                run_http(
                    listener,
                    path,
                    state,
                    #[cfg(unix)]
                    command_listener,
                    #[cfg(unix)]
                    command_socket,
                    #[cfg(unix)]
                    Arc::new(on_command),
                    shutdown_signal(),
                )
                .await?;
            }
        }
        Ok(())
    });
    shutdown_runtime(runtime, RUNTIME_SHUTDOWN_TIMEOUT);
    result
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
