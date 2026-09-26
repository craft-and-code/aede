//! Authenticated, bounded administrative work with reconnectable task status.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::{ApiError, ApiState, CatalogEvent, authorized, error, refresh_catalog, store};

const MAX_BODY: usize = 16 * 1024;
const MAX_ACTIVE: usize = 4;
const MAX_RETAINED: usize = 64;
const MAX_OUTPUT: usize = 64 * 1024;

/// Scan options; named folders extend watched roots unless replacement is explicit.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanRequest {
    pub folders: Vec<String>,
    pub replace: bool,
    pub full: bool,
    pub threads: Option<usize>,
    pub follow_symlinks: bool,
    pub include_hidden: bool,
}

/// The explicit fetch options, using the same pass selection as the CLI.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FetchRequest {
    pub targets: Vec<String>,
    pub summaries: bool,
    pub discography: bool,
    pub covers: bool,
    pub lyrics: bool,
    pub identify: bool,
    pub credits: bool,
    pub recordings: bool,
    pub portraits: bool,
    pub logos: bool,
    pub labels: bool,
    pub fanart: bool,
    pub banners: bool,
    pub images: bool,
    pub full: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub no_logo: bool,
    pub no_label_logo: bool,
    pub no_portrait: bool,
    pub no_background: bool,
    pub no_banner: bool,
    pub no_album_cover: bool,
    pub no_cdart: bool,
    pub size: Option<String>,
    pub lang: Option<String>,
}

/// A typed operation; HTTP clients cannot provide arbitrary executable arguments.
#[derive(Clone, Debug)]
pub enum JobRequest {
    Scan(ScanRequest),
    Fetch(FetchRequest),
}

impl JobRequest {
    fn kind(&self) -> &'static str {
        match self {
            Self::Scan(_) => "scan",
            Self::Fetch(_) => "identification",
        }
    }

    fn validate(&self) -> Result<(), ApiError> {
        let invalid = |message: &str| error(StatusCode::BAD_REQUEST, "invalid_parameters", message);
        match self {
            Self::Scan(scan) => {
                if scan.folders.len() > 64
                    || scan.folders.iter().any(|folder| {
                        folder.len() > 4096
                            || folder.contains('\0')
                            || !std::path::Path::new(folder).is_absolute()
                    })
                {
                    return Err(invalid(
                        "folders must contain at most 64 absolute server paths of at most 4096 bytes without NUL",
                    ));
                }
                if scan.replace && scan.folders.is_empty() {
                    return Err(invalid("replace requires at least one folder"));
                }
                if scan.threads.is_some_and(|threads| threads > 64) {
                    return Err(invalid("threads must be between 0 and 64"));
                }
            }
            Self::Fetch(fetch) => {
                if fetch.targets.len() > 64
                    || fetch.targets.iter().any(|target| {
                        target.trim().is_empty() || target.len() > 4096 || target.contains('\0')
                    })
                {
                    return Err(invalid(
                        "targets must contain at most 64 nonempty strings of at most 4096 bytes without NUL",
                    ));
                }
                if fetch.lang.as_ref().is_some_and(|lang| {
                    lang.trim().is_empty() || lang.len() > 256 || lang.contains('\0')
                }) {
                    return Err(invalid(
                        "lang must be a nonempty string of at most 256 bytes without NUL",
                    ));
                }
                if let Some(size) = &fetch.size
                    && aede_core::coverart::Size::parse(size).is_none()
                {
                    return Err(invalid("size must be 250, 500, 1200 or original"));
                }
                if !fetch.covers && (fetch.images || fetch.size.is_some()) {
                    return Err(invalid("images and size require covers"));
                }
                if fetch.banners && !fetch.logos && !fetch.fanart {
                    return Err(invalid("banners requires logos or fanart"));
                }
                if !fetch.fanart
                    && [
                        fetch.no_logo,
                        fetch.no_label_logo,
                        fetch.no_portrait,
                        fetch.no_background,
                        fetch.no_banner,
                        fetch.no_album_cover,
                        fetch.no_cdart,
                    ]
                    .into_iter()
                    .any(|excluded| excluded)
                {
                    return Err(invalid("no_* artwork exclusions require fanart"));
                }
                if fetch.fanart
                    && ((fetch.logos && fetch.no_logo) || (fetch.banners && fetch.no_banner))
                {
                    return Err(invalid(
                        "positive artwork options conflict with their exclusions",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Bounded captured output of the existing command implementation.
#[derive(Clone, Debug, Default, Serialize)]
pub struct JobOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
}

pub(super) type JobCallback =
    dyn Fn(JobRequest, Arc<AtomicBool>) -> Result<JobOutput, String> + Send + Sync;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl Status {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Serialize)]
struct Failure {
    code: &'static str,
    message: String,
}

#[derive(Clone, Serialize)]
struct Task {
    task_id: u64,
    task_kind: &'static str,
    status: Status,
    cancel_requested: bool,
    result: Option<JobOutput>,
    error: Option<Failure>,
    #[serde(skip)]
    cancellation: Arc<AtomicBool>,
}

#[derive(Default)]
struct RegistryData {
    tasks: BTreeMap<u64, Task>,
    active: usize,
}

#[derive(Default)]
pub(super) struct JobRegistry {
    data: Mutex<RegistryData>,
    finished: tokio::sync::Notify,
}

impl JobRegistry {
    fn insert(&self, task_id: u64, task_kind: &'static str) -> Result<Arc<AtomicBool>, ApiError> {
        let mut data = self.data.lock().map_err(|_| registry_error())?;
        if data.active >= MAX_ACTIVE {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "task_limit",
                "administrative task capacity reached",
            ));
        }
        if data.tasks.len() >= MAX_RETAINED
            && let Some(oldest) = data
                .tasks
                .iter()
                .find(|(_, task)| task.status.is_terminal())
                .map(|(&id, _)| id)
        {
            data.tasks.remove(&oldest);
        }
        let cancellation = Arc::new(AtomicBool::new(false));
        data.tasks.insert(
            task_id,
            Task {
                task_id,
                task_kind,
                status: Status::Queued,
                cancel_requested: false,
                result: None,
                error: None,
                cancellation: cancellation.clone(),
            },
        );
        data.active += 1;
        Ok(cancellation)
    }

    fn get(&self, task_id: u64) -> Result<Task, ApiError> {
        self.data
            .lock()
            .map_err(|_| registry_error())?
            .tasks
            .get(&task_id)
            .cloned()
            .ok_or_else(|| {
                error(
                    StatusCode::NOT_FOUND,
                    "task_not_found",
                    "task is unknown or no longer retained",
                )
            })
    }

    fn cancel(&self, task_id: u64) -> Result<(), ApiError> {
        let mut data = self.data.lock().map_err(|_| registry_error())?;
        let task = data.tasks.get_mut(&task_id).ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                "task_not_found",
                "task is unknown or no longer retained",
            )
        })?;
        if task.status.is_terminal() {
            return Err(error(
                StatusCode::CONFLICT,
                "task_not_cancellable",
                "task has already finished",
            ));
        }
        task.cancel_requested = true;
        task.cancellation.store(true, Ordering::Release);
        Ok(())
    }

    fn running(&self, task_id: u64) -> Result<(), ApiError> {
        if let Some(task) = self
            .data
            .lock()
            .map_err(|_| registry_error())?
            .tasks
            .get_mut(&task_id)
        {
            task.status = Status::Running;
        }
        Ok(())
    }

    fn finish(&self, task_id: u64, outcome: Result<JobOutput, String>) -> Result<Task, ApiError> {
        let mut data = self.data.lock().map_err(|_| registry_error())?;
        let task = data.tasks.get_mut(&task_id).ok_or_else(registry_error)?;
        match outcome {
            Ok(mut output) => {
                for text in [&mut output.stdout, &mut output.stderr] {
                    if text.len() > MAX_OUTPUT {
                        let mut at = MAX_OUTPUT;
                        while !text.is_char_boundary(at) {
                            at -= 1;
                        }
                        text.truncate(at);
                        output.output_truncated = true;
                    }
                }
                task.status = if output.exit_code == 130 && task.cancel_requested {
                    Status::Cancelled
                } else if output.exit_code == 0 {
                    Status::Completed
                } else {
                    Status::Failed
                };
                task.result = Some(output);
                if task.status != Status::Completed {
                    task.error = Some(Failure {
                        code: if task.status == Status::Cancelled {
                            "task_cancelled"
                        } else {
                            "command_failed"
                        },
                        message: if task.status == Status::Cancelled {
                            "task cancelled by user"
                        } else {
                            "administrative command failed; inspect the authenticated task result"
                        }
                        .into(),
                    });
                }
            }
            Err(message) => {
                task.status = Status::Failed;
                task.error = Some(Failure {
                    code: "command_failed",
                    message: message.chars().take(1024).collect(),
                });
            }
        }
        let finished = task.clone();
        data.active -= 1;
        self.finished.notify_waiters();
        Ok(finished)
    }
}

fn registry_error() -> ApiError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "task_error",
        "administrative task state is unavailable",
    )
}

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/admin/v1/fetch", post(fetch_route))
        .route("/api/admin/v1/tasks/:id", get(task_route))
        .route("/api/admin/v1/tasks/:id/cancel", post(cancel_route))
}

fn check_request(state: &ApiState, request: &Request) -> Result<(), ApiError> {
    let admin = state
        .admin
        .as_ref()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "not_found", "unknown API route"))?;
    if !authorized(request.headers(), &admin.token) {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "administrative token required",
        ));
    }
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "administrative tasks accept no query parameters",
        ));
    }
    Ok(())
}

async fn body(request: Request, limit: usize) -> Result<axum::body::Bytes, ApiError> {
    tokio::time::timeout(Duration::from_secs(1), to_bytes(request.into_body(), limit))
        .await
        .map_err(|_| {
            error(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "request body did not finish",
            )
        })?
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_body",
                "request body is malformed or too large",
            )
        })
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
    if bytes
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        != Some(b'{')
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_body",
            "expected a JSON object",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_body",
            "expected a JSON object with supported fields and types",
        )
    })
}

pub(super) async fn scan_route(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    let (parts, incoming) = request.into_parts();
    let bytes = body(Request::new(incoming), MAX_BODY).await?;
    if bytes.is_empty() {
        return super::admin_scan(State(state), Request::from_parts(parts, Body::empty()))
            .await
            .map(IntoResponse::into_response);
    }
    enqueue(state, JobRequest::Scan(decode(&bytes)?))
}

async fn fetch_route(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    let bytes = body(request, MAX_BODY).await?;
    enqueue(state, JobRequest::Fetch(decode(&bytes)?))
}

#[derive(Serialize)]
struct Accepted {
    task_id: u64,
    status: &'static str,
    status_url: String,
}

fn enqueue(state: ApiState, request: JobRequest) -> Result<Response, ApiError> {
    request.validate()?;
    let admin = state.admin.as_ref().ok_or_else(registry_error)?;
    let run = admin.job.clone();
    let task_id = state.next_task_id.fetch_add(1, Ordering::Relaxed);
    let kind = request.kind();
    let cancellation = state.jobs.insert(task_id, kind)?;
    tokio::spawn(run_job(state, task_id, request, cancellation, run));
    Ok((
        StatusCode::ACCEPTED,
        Json(Accepted {
            task_id,
            status: "queued",
            status_url: format!("/api/admin/v1/tasks/{task_id}"),
        }),
    )
        .into_response())
}

async fn run_job(
    state: ApiState,
    task_id: u64,
    request: JobRequest,
    cancellation: Arc<AtomicBool>,
    run: Arc<JobCallback>,
) {
    let kind = request.kind();
    let outcome = if state.jobs.running(task_id).is_err() {
        Err("cannot start administrative task".into())
    } else {
        let _ = state.events.send(CatalogEvent::TaskStarted {
            task_id,
            task_kind: kind,
        });
        tokio::task::spawn_blocking(move || {
            if cancellation.load(Ordering::Acquire) {
                return Ok(JobOutput {
                    exit_code: 130,
                    ..Default::default()
                });
            }
            run(request, cancellation)
        })
        .await
        .unwrap_or_else(|_| Err("administrative command worker failed".into()))
    };
    if let Some(admin) = &state.admin {
        let path = store::catalog_path(&admin.data_dir);
        let mut known = *state.loaded_stamp.read().await;
        refresh_catalog(&path, &state, &mut known, &mut None).await;
    }
    match state.jobs.finish(task_id, outcome) {
        Ok(task) if task.status == Status::Completed => {
            let scanned_at = state
                .catalog
                .read()
                .await
                .as_ref()
                .map(|catalog| catalog.scanned_at);
            let _ = state.events.send(CatalogEvent::TaskCompleted {
                task_id,
                task_kind: kind,
                scanned_at,
            });
        }
        Ok(task) => {
            if let Some(failure) = task.error {
                let _ = state.events.send(CatalogEvent::TaskFailed {
                    task_id,
                    task_kind: kind,
                    code: failure.code,
                    // Public activity must not disclose diagnostics reserved
                    // for the authenticated task result.
                    message: if task.status == Status::Cancelled {
                        "task cancelled by user"
                    } else {
                        "administrative command failed; inspect the authenticated task result"
                    }
                    .into(),
                });
            }
        }
        Err(_) => eprintln!("Aède administrative task {task_id} could not publish its final state"),
    }
}

fn task_id(raw: &str) -> Result<u64, ApiError> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_task_id",
            "task ID must be a positive whole number",
        ));
    }
    raw.parse::<u64>().ok().filter(|id| *id > 0).ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            "invalid_task_id",
            "task ID must be a positive whole number",
        )
    })
}

async fn task_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Json<Task>, ApiError> {
    check_request(&state, &request)?;
    body(request, 0).await?;
    Ok(Json(state.jobs.get(task_id(&id)?)?))
}

async fn cancel_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    body(request, 0).await?;
    let task_id = task_id(&id)?;
    state.jobs.cancel(task_id)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(Accepted {
            task_id,
            status: "cancel_requested",
            status_url: format!("/api/admin/v1/tasks/{task_id}"),
        }),
    )
        .into_response())
}

pub(super) async fn wait_for_jobs(state: &ApiState) {
    loop {
        let finished = state.jobs.finished.notified();
        match state.jobs.data.lock() {
            Ok(data) if data.active == 0 => return,
            Ok(_) => {}
            Err(_) => {
                eprintln!("Aède could not inspect administrative tasks during shutdown");
                return;
            }
        }
        finished.await;
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
