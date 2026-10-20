//! Explicit administrator-owned UPnP sessions, separate from PCM/history acknowledgement.
use super::*;
use std::net::Ipv4Addr;
use std::sync::Mutex as BlockingMutex;
use std::sync::atomic::AtomicBool;

pub(super) struct NetworkAudio {
    run: BlockingMutex<Option<Run>>,
    discovery: Arc<Semaphore>,
}
impl Default for NetworkAudio {
    fn default() -> Self {
        Self {
            run: BlockingMutex::new(None),
            discovery: Arc::new(Semaphore::new(1)),
        }
    }
}
struct CancellationGuard(Arc<AtomicBool>);
impl Drop for CancellationGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
struct Run {
    id: u64,
    owner: String,
    cancelled: Arc<AtomicBool>,
    status: &'static str,
    last_seen: Instant,
}
impl NetworkAudio {
    fn expired(&self, id: u64) -> bool {
        self.run
            .lock()
            .map(|held| {
                held.as_ref().is_none_or(|run| {
                    run.id != id || run.last_seen.elapsed() > Duration::from_secs(5)
                })
            })
            .unwrap_or(true)
    }
    fn stop(&self, owner: &str, id: u64) -> Result<serde_json::Value, ApiError> {
        let mut held = self.run.lock().map_err(|_| unavailable())?;
        let run = held
            .as_mut()
            .filter(|run| run.id == id && run.owner == owner)
            .ok_or_else(|| {
                error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    "network session not found",
                )
            })?;
        if run.status == "running" {
            run.cancelled.store(true, Ordering::Release);
            run.status = "stopping";
        }
        Ok(serde_json::json!({"id":run.id,"status":run.status}))
    }
    fn snapshot(&self, owner: &str) -> Result<serde_json::Value, ApiError> {
        let mut run = self.run.lock().map_err(|_| unavailable())?;
        Ok(match run.as_mut().filter(|run| run.owner == owner) {
            Some(run) => {
                run.last_seen = Instant::now();
                serde_json::json!({"id": run.id, "status": run.status})
            }
            None => serde_json::Value::Null,
        })
    }
}
fn unavailable() -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "network_audio_unavailable",
        "network audio unavailable",
    )
}
fn owner(state: &ApiState, request: &Request) -> Result<auth::Principal, ApiError> {
    require_admin(state, request)?;
    let principal = auth::request_principal(request)?;
    if principal.role != aede_core::accounts::Role::Administrator {
        return Err(auth::unauthorized());
    }
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(super::invalid_query(
            "network audio accepts no query parameters",
        ));
    }
    Ok(principal)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Discovery {
    bind: Ipv4Addr,
    #[serde(default)]
    all: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inspection {
    bind: Ipv4Addr,
    device: String,
}
#[path = "network_audio_profile.rs"]
mod profile;
use profile::Playback;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stop {
    id: u64,
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(bytes).map_err(|_| super::invalid_query("invalid network audio request"))
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/admin/v1/network-audio", get(status))
        .route("/api/admin/v1/network-audio/discover", post(discover))
        .route("/api/admin/v1/network-audio/play", post(play))
        .route("/api/admin/v1/network-audio/inspect", post(inspect))
        .route("/api/admin/v1/network-audio/stop", post(stop))
}
async fn status(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = owner(&state, &request)?;
    super::jobs::administrative_body(&state, request, 0).await?;
    Ok(Json(
        state
            .network_audio
            .snapshot(auth::session_token(&principal))?,
    ))
}
async fn discover(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = owner(&state, &request)?;
    let (_, bytes) = super::jobs::administrative_body(&state, request, 1024).await?;
    let input: Discovery = decode(&bytes)?;
    let permit = state
        .network_audio
        .discovery
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            error(
                StatusCode::CONFLICT,
                "network_audio_busy",
                "device discovery is busy",
            )
        })?;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let mut items = Vec::new();
        let mut failures = Vec::new();
        match aede_devices::discover(input.bind) {
            Ok(rows) => {
                for row in rows {
                    for protocol in row.protocols {
                        if protocol == "upnp" || (input.all && protocol == "openhome") {
                            items.push(serde_json::json!({"name": row.name, "endpoint":row.location, "protocol":protocol}));
                        }
                    }
                }
            }
            Err(_) => failures.push("UPnP/OpenHome discovery failed"),
        }
        if input.all {
            match aede_devices::discover_cast(input.bind) {
                Ok(rows) => items.extend(rows.into_iter().map(|row| serde_json::json!({"name":row.name,"endpoint":row.location,"protocol":"googlecast"}))),
                Err(_) => failures.push("Cast discovery failed"),
            }
        }
        if items.is_empty() && !failures.is_empty() { return Err("discovery failed"); }
        Ok(serde_json::json!({"items":items,"errors":failures}))
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "network_audio_discovery_failed",
            "UPnP discovery failed; check the selected LAN interface",
        )
    })?;
    auth::recheck(&state, &principal)?;
    Ok(Json(result))
}
async fn inspect(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = owner(&state, &request)?;
    let (_, bytes) = super::jobs::administrative_body(&state, request, 8192).await?;
    let input: Inspection = decode(&bytes)?;
    let _permit = state
        .network_audio
        .discovery
        .clone()
        .try_acquire_owned()
        .map_err(|_| unavailable())?;
    if input.device.len() > 4096 {
        return Err(super::invalid_query("invalid Cast endpoint"));
    }
    let fingerprint = tokio::task::spawn_blocking(move || {
        let _permit = _permit;
        aede_devices::inspect_cast_certificate(input.bind, &input.device)
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| super::invalid_query("Cast certificate inspection failed"))?;
    auth::recheck(&state, &principal)?;
    Ok(Json(serde_json::json!({"sha256": fingerprint})))
}
async fn play(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = owner(&state, &request)?;
    let (_, bytes) = super::jobs::administrative_body(&state, request, 1024 * 1024).await?;
    let input: Playback = decode(&bytes)?;
    if input.tracks.is_empty() || input.tracks.len() > 64 || input.device.len() > 4096 {
        return Err(super::invalid_query(
            "select 1 to 64 catalog tracks and one UPnP device",
        ));
    }
    let options = input
        .options()
        .map_err(|_| super::invalid_query("invalid network device options"))?;
    let sources = {
        let catalog = state.catalog.read().await;
        let catalog = catalog.as_ref().ok_or_else(unavailable)?;
        input
            .tracks
            .iter()
            .map(|raw| {
                if raw.len() > 16384 {
                    return Err(super::invalid_query("invalid track reference"));
                }
                let reference = EntityRef::parse_token(raw)
                    .ok_or_else(|| super::invalid_query("invalid track reference"))?;
                super::playback_api::source_from_catalog(catalog, &reference).map_err(|_| {
                    super::invalid_query("track source unavailable; rescan the library")
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    // Source validation performs filesystem IO off the async executor.
    let paths = tokio::task::spawn_blocking(move || {
        sources
            .iter()
            .map(|source| {
                super::playback_api::validate_source(source).map_err(|_| "track source changed")?;
                Ok(source.path.clone())
            })
            .collect::<Result<Vec<_>, &str>>()
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| super::invalid_query("track source changed; rescan the library"))?;
    auth::recheck(&state, &principal)?;
    let id = state.next_task_id.fetch_add(1, Ordering::Relaxed);
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut held = state.network_audio.run.lock().map_err(|_| unavailable())?;
        if held
            .as_ref()
            .is_some_and(|run| matches!(run.status, "running" | "stopping"))
        {
            return Err(error(
                StatusCode::CONFLICT,
                "network_audio_busy",
                "a network session is already active",
            ));
        }
        *held = Some(Run {
            id,
            owner: auth::session_token(&principal).into(),
            cancelled: cancelled.clone(),
            status: "running",
            last_seen: Instant::now(),
        });
    }
    tokio::spawn(supervise(
        state.clone(),
        principal,
        id,
        cancelled,
        options,
        paths,
    ));
    Ok(Json(serde_json::json!({"id":id,"status":"running"})))
}
fn supervise(
    state: ApiState,
    principal: auth::Principal,
    id: u64,
    cancelled: Arc<AtomicBool>,
    options: aede_devices::CastOptions,
    paths: Vec<PathBuf>,
) -> impl Future<Output = ()> + Send {
    // Capture shutdown before spawning: an accepted run must not lose a signal
    // delivered before this future receives its first poll.
    let mut shutdown = state.shutdown.subscribe();
    let cancel_on_drop = CancellationGuard(cancelled.clone());
    async move {
        let _cancel_on_drop = cancel_on_drop;
        if !matches!(
            shutdown.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ) {
            cancelled.store(true, Ordering::Release);
        }
        let flag = cancelled.clone();
        let mut worker = tokio::task::spawn_blocking(move || {
            aede_devices::cast_controlled(options, paths, flag)
        });
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        let mut shutdown_seen = false;
        let outcome = loop {
            tokio::select! {
                result = &mut worker => break result,
                _ = shutdown.recv(), if !shutdown_seen => { shutdown_seen = true; cancelled.store(true, Ordering::Release); },
                _ = interval.tick() => {
                    let expired = state.network_audio.expired(id);
                    if expired { cancelled.store(true, Ordering::Release); }

                    let check_state = state.clone(); let check_principal = principal.clone();
                    if !matches!(tokio::task::spawn_blocking(move || auth::recheck(&check_state, &check_principal)).await, Ok(Ok(()))) {
                        cancelled.store(true, Ordering::Release);
                    }
                }
            }
        };
        if let Ok(mut held) = state.network_audio.run.lock()
            && let Some(run) = held.as_mut().filter(|run| run.id == id)
        {
            run.status = match (
                matches!(outcome, Ok(Ok(()))),
                cancelled.load(Ordering::Acquire),
            ) {
                (true, true) => "stopped",
                (true, false) => "completed",
                (false, true) => "stop_unconfirmed",
                (false, false) => "failed",
            };
        }
    }
}
async fn stop(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let principal = owner(&state, &request)?;
    let (_, bytes) = super::jobs::administrative_body(&state, request, 1024).await?;
    let input: Stop = decode(&bytes)?;
    Ok(Json(
        state
            .network_audio
            .stop(auth::session_token(&principal), input.id)?,
    ))
}

#[cfg(test)]
#[path = "network_audio_tests.rs"]
mod tests;
