//! Server startup, catalog reloads and graceful shutdown.

use super::*;

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

pub(super) async fn watch_catalog(path: PathBuf, state: ApiState) {
    let mut known = *state.loaded_stamp.read().await;
    let mut reported_failure: Option<Option<(SystemTime, u64)>> = None;
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut shutdown = state.shutdown.subscribe();
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
    let sender = state.shutdown.clone();
    let cleanup_shutdown = state.shutdown.clone();
    let address = listener.local_addr()?;
    let result = axum::serve(listener, router(state.clone(), address))
        .with_graceful_shutdown(async move {
            shutdown.await;
            let _ = sender.send(());
        })
        .await;
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

/// Starts the loopback API until the process is stopped.
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
    #[cfg(not(unix))]
    let _ = on_command;
    #[cfg(unix)]
    let _server_lock = delegation::ServerLock::acquire(data_dir)?;
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
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        #[cfg(unix)]
        let (command_listener, command_socket) = delegation::bind_command_socket(data_dir)?;
        let (events, _) = broadcast::channel(32);
        let (shutdown, _) = broadcast::channel(1);
        let admin = admin_token.map(|token| Admin {
            token,
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
            next_task_id: Arc::new(AtomicU64::new(1)),
            websocket_slots: Arc::new(Semaphore::new(MAX_WEBSOCKETS)),
            inspection_slots: Arc::new(Semaphore::new(2)),
            jobs: Arc::new(jobs::JobRegistry::default()),
            #[cfg(unix)]
            tasks: Arc::new(delegation::TaskRegistry::default()),
        };
        let address = listener.local_addr()?;
        on_ready(address);
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
        Ok(())
    })
}
