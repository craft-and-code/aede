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

use super::{ApiError, ApiState, CatalogEvent, error, refresh_catalog, require_admin, store};

#[path = "job_check_progress.rs"]
mod check_progress;
pub use check_progress::CheckRequest;

const MAX_BODY: usize = 16 * 1024;
const MAX_ACTIVE: usize = 4;
const MAX_RETAINED: usize = 64;
const MAX_OUTPUT: usize = 64 * 1024;

/// Scan options; named folders extend watched roots unless replacement is explicit.
///
/// Missing JSON fields use these defaults. The administrative HTTP endpoint
/// validates paths and bounds before invoking the server's job callback.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanRequest {
    /// Up to 64 absolute server-side folders, 4096 bytes each and without NUL.
    /// Empty means rescan the already watched roots.
    pub folders: Vec<String>,
    /// Explicitly unavailable mount roots; preserve retained catalog entries without traversal.
    pub unavailable_folders: Vec<String>,
    /// Replace watched roots instead of adding; requires nonempty `folders`.
    pub replace: bool,
    /// Reread tags rather than reusing unchanged files; keep saved conclusions.
    pub full: bool,
    /// Tag-reading concurrency from 0 to 64; `None` and zero choose automatically.
    pub threads: Option<usize>,
    /// Follow symbolic links during discovery; disabled by default.
    pub follow_symlinks: bool,
    /// Include hidden entries during discovery; disabled by default.
    pub include_hidden: bool,
}

/// The explicit fetch options, using the same pass selection as the CLI.
///
/// Missing booleans default to false. No pass flags selects ordinary MusicBrainz
/// identification, so a default request is not a no-op. HTTP validates option
/// dependencies before the callback; optional bounded client keys apply only
/// to this job. Full details are in `docs/server/jobs.md`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FetchRequest {
    /// Optional client-owned keys, confined to the existing trusted admin endpoint.
    pub service_keys: Option<super::FetchServiceKeys>,
    /// Up to 64 nonempty names or server-folder paths, 4096 bytes each, no NUL.
    /// Empty selects the CLI's ordinary all-catalog scope for the chosen passes.
    pub targets: Vec<String>,
    /// Exact local artist reference for a summaries-only pass; excludes name/folder targets.
    pub artist: Option<String>,
    /// Run ordinary MusicBrainz identification before the selected secondary passes.
    pub musicbrainz: bool,
    /// Fetch attributed artist/album prose.
    pub summaries: bool,
    /// Fetch artist discographies for later missing-release inspection.
    pub discography: bool,
    /// Fetch cover-art information; image downloads additionally require `images`.
    pub covers: bool,
    /// Explicitly fetch lyrics; identification never enables this implicitly.
    pub lyrics: bool,
    /// Include the ordinary identification pass alongside selected passes.
    pub identify: bool,
    /// Fetch edition, recording and work credits through the scoped CLI pipeline.
    pub credits: bool,
    /// Fetch recording-level identification/information.
    pub recordings: bool,
    /// Fetch supported artist portraits.
    pub portraits: bool,
    /// Fetch supported artist/label logos.
    pub logos: bool,
    /// Fetch record-label information.
    pub labels: bool,
    /// Fetch identifier-linked Discogs label biographies explicitly.
    pub discogs: bool,
    /// Enable the Fanart.tv artwork families and their explicit exclusions.
    pub fanart: bool,
    /// Include banners; requires `logos` or `fanart` and conflicts with `no_banner`.
    pub banners: bool,
    /// Download derivative cover images; requires `covers`.
    pub images: bool,
    /// Repeat lookups that would otherwise reuse stored complete results.
    pub full: bool,
    /// Preview the selected work without contacting external services.
    pub dry_run: bool,
    /// Explicitly accept a large-run confirmation; HTTP never prompts interactively.
    pub yes: bool,
    /// Exclude Fanart.tv artist logos; requires `fanart` and conflicts with `logos`.
    pub no_logo: bool,
    /// Exclude Fanart.tv label logos; requires `fanart`.
    pub no_label_logo: bool,
    /// Exclude Fanart.tv artist portraits; requires `fanart`.
    pub no_portrait: bool,
    /// Exclude Fanart.tv artist backgrounds; requires `fanart`.
    pub no_background: bool,
    /// Exclude Fanart.tv banners; requires `fanart` and conflicts with `banners`.
    pub no_banner: bool,
    /// Exclude Fanart.tv album covers; requires `fanart`.
    pub no_album_cover: bool,
    /// Exclude Fanart.tv disc artwork; requires `fanart`.
    pub no_cdart: bool,
    /// Cover size `250`, `500`, `1200`, `original` (`full` aliases original).
    /// Requires `covers`; `None` keeps the CLI default.
    pub size: Option<String>,
    /// Nonempty prose-language selection, up to 256 bytes and without NUL.
    /// `None` keeps the CLI's configured language behavior.
    pub lang: Option<String>,
}

/// Catalog-resolved local media work; HTTP accepts release references instead of paths.
#[derive(Clone, Debug)]
pub struct MediaJobRequest {
    /// Exact catalog folders resolved by the server before admission.
    pub targets: Vec<String>,
    /// Explicitly repeat cached work (force analysis, full spectrum redraw).
    pub force: bool,
    /// Bounded worker concurrency, zero selecting the ordinary automatic policy.
    pub threads: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaSelection {
    release: String,
    #[serde(default)]
    force: bool,
    threads: Option<usize>,
}

/// Fixed whole-catalog stages; clients never supply paths or executable arguments.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStep {
    /// Reuse or create FlacCompagnon album reports.
    Analyze,
    /// Reuse or create track spectrograms.
    Spectrum,
    /// Publish catalog-ordered playlists in album folders.
    M3u,
    /// Explicitly obtain missing lyrics through the existing provider pass.
    Lyrics,
}
/// A sequential, cancellable plan owned by one Aède server.
#[derive(Clone, Debug)]
pub struct GenerationRequest {
    /// One to four distinct stages in the requested order.
    pub steps: Vec<GenerationStep>,
    /// Current watched roots captured by the server, never accepted from an HTTP body.
    pub roots: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerationInput {
    steps: Vec<GenerationStep>,
}

/// A typed operation; HTTP clients cannot provide arbitrary executable arguments.
#[derive(Clone, Debug)]
pub enum JobRequest {
    /// Whole-catalog derivative work, serialized in the existing CLI worker.
    Generate(GenerationRequest),
    /// Exact catalogued album files, never a client-supplied path or folder expansion.
    Check(CheckRequest),
    /// A local scan of watched/newly supplied folders.
    Scan(ScanRequest),
    /// An explicitly requested online pass, or an offline `dry_run` preview.
    Fetch(FetchRequest),
    /// Explicit local acoustic analysis, with ordinary CLI persistence.
    Analyze(MediaJobRequest),
    /// Explicit local spectrogram generation, with ordinary CLI caching.
    Spectrum(MediaJobRequest),
}

impl JobRequest {
    fn kind(&self) -> &'static str {
        match self {
            Self::Generate(plan) => match plan.steps.as_slice() {
                [GenerationStep::Analyze] => "library_generation_analyze",
                [GenerationStep::Spectrum] => "library_generation_spectrum",
                [GenerationStep::M3u] => "library_generation_m3u",
                [GenerationStep::Lyrics] => "library_generation_lyrics",
                _ => "library_generation",
            },
            Self::Check(_) => "album_decode_test",
            Self::Scan(_) => "scan",
            Self::Fetch(_) => "identification",
            Self::Analyze(_) => "analysis",
            Self::Spectrum(_) => "spectrum",
        }
    }

    fn validate(&self) -> Result<(), ApiError> {
        let invalid = |message: &str| error(StatusCode::BAD_REQUEST, "invalid_parameters", message);
        match self {
            Self::Generate(plan) => {
                let unique: std::collections::BTreeSet<_> = plan.steps.iter().collect();
                if plan.roots.is_empty()
                    || plan.roots.len() > 64
                    || plan.roots.iter().any(|root| {
                        root.len() > 4096
                            || root.contains('\0')
                            || !std::path::Path::new(root).is_absolute()
                    })
                {
                    return Err(invalid("generation needs current watched roots"));
                }
                if plan.steps.is_empty() || plan.steps.len() > 4 || unique.len() != plan.steps.len()
                {
                    return Err(invalid("generation needs one to four distinct stages"));
                }
                if !cfg!(feature = "fetch") && plan.steps.contains(&GenerationStep::Lyrics) {
                    return Err(invalid("lyrics generation requires the fetch feature"));
                }
            }
            Self::Check(check) => {
                let files = &check.files;
                if files.is_empty()
                    || files.len() > 1024
                    || files.iter().map(String::len).sum::<usize>() > 128_000
                    || files.iter().any(|file| {
                        file.len() > 4096
                            || file.contains('\0')
                            || !std::path::Path::new(file).is_absolute()
                    })
                {
                    return Err(invalid("decode test needs bounded exact catalog files"));
                }
            }
            Self::Scan(scan) => {
                if scan.folders.len() > 64
                    || scan.unavailable_folders.len() > 64
                    || scan
                        .folders
                        .iter()
                        .chain(&scan.unavailable_folders)
                        .any(|folder| {
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
            Self::Analyze(media) | Self::Spectrum(media) => {
                if media.targets.len() != 1
                    || media.targets.iter().any(|target| {
                        target.len() > 4096
                            || target.contains('\0')
                            || !std::path::Path::new(target).is_absolute()
                    })
                    || media.threads.is_some_and(|threads| threads > 64)
                {
                    return Err(invalid(
                        "media work needs one catalog folder and 0 to 64 threads",
                    ));
                }
            }
            Self::Fetch(fetch) => {
                if let Some(reference) = &fetch.artist
                    && (reference.len() > 4096
                        || reference.contains('\0')
                        || !reference.starts_with("artist:")
                        || reference.len() == 7
                        || !fetch.targets.is_empty()
                        || !fetch.summaries
                        || [
                            fetch.musicbrainz,
                            fetch.discography,
                            fetch.covers,
                            fetch.lyrics,
                            fetch.identify,
                            fetch.credits,
                            fetch.recordings,
                            fetch.portraits,
                            fetch.logos,
                            fetch.labels,
                            fetch.discogs,
                            fetch.fanart,
                        ]
                        .into_iter()
                        .any(|pass| pass))
                {
                    return Err(invalid(
                        "artist requires an exact artist reference and summaries only",
                    ));
                }
                if fetch
                    .service_keys
                    .as_ref()
                    .is_some_and(|keys| !keys.valid())
                {
                    return Err(invalid(
                        "service keys must be nonempty, at most 1024 bytes and without control characters",
                    ));
                }
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
                    && ((fetch.logos && (fetch.no_logo || fetch.no_label_logo))
                        || (fetch.banners && fetch.no_banner))
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
    /// CLI exit code: zero succeeds; a cancelled callback normally returns 130.
    pub exit_code: i32,
    /// Standard output, retained at most 64 KiB on a UTF-8 boundary.
    pub stdout: String,
    /// Standard error, retained at most 64 KiB on a UTF-8 boundary.
    pub stderr: String,
    /// Either captured stream was shortened; retained output is incomplete.
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

/// Real per-file progress of native background work, without private rule labels.
#[derive(Clone, Default, Serialize)]
pub(super) struct NativeProgress {
    pub(super) phase: &'static str,
    pub(super) done: usize,
    pub(super) total: usize,
    pub(super) analyzed: usize,
    pub(super) reused: usize,
    pub(super) failed: usize,
}
#[derive(Clone, Serialize)]
struct Task {
    task_id: u64,
    task_kind: &'static str,
    status: Status,
    cancel_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<NativeProgress>,
    result: Option<JobOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    partial_result: Option<JobOutput>,
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
        if (task_kind.starts_with("library_generation") && data.active > 0)
            || data.tasks.values().any(|task| {
                task.task_kind.starts_with("library_generation") && !task.status.is_terminal()
            })
        {
            return Err(error(
                StatusCode::CONFLICT,
                "generation_task_active",
                "whole-library generation excludes concurrent administrative work",
            ));
        }
        if task_kind == "mood_features"
            && data
                .tasks
                .values()
                .any(|task| task.task_kind == task_kind && !task.status.is_terminal())
        {
            return Err(error(
                StatusCode::CONFLICT,
                "feature_task_active",
                "acoustic extraction is already active",
            ));
        }
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
                progress: None,
                result: None,
                partial_result: None,
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
                if task.status != Status::Cancelled {
                    task.partial_result = None;
                }
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
        .route(
            "/api/admin/v1/mood-features",
            post(super::audio_features::start),
        )
        .route("/api/admin/v1/generate", post(generation_route))
        .route("/api/admin/v1/lyrics", post(lyrics_route))
        .route("/api/admin/v1/check", post(check_route))
        .route("/api/admin/v1/analyze", post(analyze_route))
        .route("/api/admin/v1/spectrum", post(spectrum_route))
        .route("/api/admin/v1/tasks", get(task_list_route))
        .route("/api/admin/v1/tasks/:id", get(task_route))
        .route("/api/admin/v1/tasks/:id/cancel", post(cancel_route))
}

pub(super) fn check_request(state: &ApiState, request: &Request) -> Result<(), ApiError> {
    require_admin(state, request)?;
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "administrative tasks accept no query parameters",
        ));
    }
    Ok(())
}

pub(super) async fn body(request: Request, limit: usize) -> Result<axum::body::Bytes, ApiError> {
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

/// Body reception yields to logout/password/role changes. Admit the operation
/// only after rechecking the captured login; accepted jobs remain server-owned.
pub(super) async fn administrative_body(
    state: &ApiState,
    request: Request,
    limit: usize,
) -> Result<(axum::http::request::Parts, axum::body::Bytes), ApiError> {
    check_request(state, &request)?;
    let (parts, incoming) = request.into_parts();
    let bytes = body(Request::new(incoming), limit).await?;
    if let Some(principal) = parts.extensions.get::<super::auth::Principal>().cloned() {
        let Json(()) = super::inspection::inspect(state.clone(), move |state| {
            super::auth::recheck(state, &principal)
        })
        .await?;
    }
    Ok((parts, bytes))
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
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
    let (parts, bytes) = administrative_body(&state, request, MAX_BODY).await?;
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
    let (_, bytes) = administrative_body(&state, request, MAX_BODY).await?;
    enqueue(state, JobRequest::Fetch(decode(&bytes)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LyricsSelection {
    track: String,
}
async fn generation_route(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    let principal = request
        .extensions()
        .get::<super::auth::Principal>()
        .cloned();
    let input: GenerationInput = decode(&body(request, MAX_BODY).await?)?;
    let roots = super::inspection::inspect(state.clone(), move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(super::unavailable)?;
        if let Some(principal) = principal.as_ref() {
            super::auth::recheck(state, principal)?;
        }
        Ok(catalog.roots.clone())
    })
    .await?
    .0;
    enqueue(
        state,
        JobRequest::Generate(GenerationRequest {
            steps: input.steps,
            roots,
        }),
    )
}

async fn lyrics_route(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    let principal = request
        .extensions()
        .get::<super::auth::Principal>()
        .cloned();
    let selected: LyricsSelection = decode(&body(request, MAX_BODY).await?)?;
    if selected.track.len() > super::MAX_REFERENCE_BYTES {
        return Err(super::invalid_query("track reference too long"));
    }
    let target = super::inspection::inspect(state.clone(), move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(super::unavailable)?;
        let requested = super::EntityRef::parse_token(&selected.track)
            .ok_or_else(|| super::invalid_query("invalid track"))?;
        let source = super::playback_api::source_from_catalog(catalog, &requested)
            .map_err(|_| super::invalid_query("current track source unavailable"))?;
        super::playback_api::validate_source(&source)
            .map_err(|_| super::invalid_query("track source changed; rescan first"))?;
        if let Some(principal) = &principal {
            super::auth::recheck(state, principal)?;
        }
        Ok(source.file.path)
    })
    .await?
    .0;
    enqueue(
        state,
        JobRequest::Fetch(FetchRequest {
            lyrics: true,
            targets: vec![target],
            yes: true,
            ..Default::default()
        }),
    )
}

async fn analyze_route(state: State<ApiState>, request: Request) -> Result<Response, ApiError> {
    media_route(state, request, false).await
}
async fn spectrum_route(state: State<ApiState>, request: Request) -> Result<Response, ApiError> {
    media_route(state, request, true).await
}
async fn media_route(
    State(state): State<ApiState>,
    request: Request,
    spectrum: bool,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    let principal = request
        .extensions()
        .get::<super::auth::Principal>()
        .cloned();
    let bytes = body(request, MAX_BODY).await?;
    let selected: MediaSelection = decode(&bytes)?;
    if selected.release.len() > super::MAX_REFERENCE_BYTES {
        return Err(super::invalid_query("release reference is too long"));
    }
    let request = super::inspection::inspect(state.clone(), move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(super::unavailable)?;
        let id = super::catalog::filter_reference(
            catalog,
            &selected.release,
            aede_core::model::EntityKind::Release,
        )?;
        let release = catalog.release(id).ok_or_else(super::unavailable)?;
        if let Some(principal) = principal.as_ref() {
            super::auth::recheck(state, principal)?;
        }
        Ok(MediaJobRequest {
            targets: vec![release.folder.clone()],
            force: selected.force,
            threads: selected.threads,
        })
    })
    .await?
    .0;
    enqueue(
        state,
        if spectrum {
            JobRequest::Spectrum(request)
        } else {
            JobRequest::Analyze(request)
        },
    )
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
    let request = match request {
        JobRequest::Check(check) => JobRequest::Check(check_progress::bind(&state, task_id, check)),
        other => other,
    };
    let (outcome, _scan_guard) = if state.jobs.running(task_id).is_err() {
        (Err("cannot start administrative task".into()), None)
    } else {
        let _ = state.events.send(CatalogEvent::TaskStarted {
            task_id,
            task_kind: kind,
        });
        let _ = state.events.send(CatalogEvent::TaskProgress {
            task_id,
            task_kind: kind,
            phase: "running",
            // Fetches and asynchronous scans do not know their amount of work
            // before the existing CLI command has inspected its catalog. Zero
            // is an explicit indeterminate progress value, not completion.
            done: 0,
            total: 0,
        });
        let scan_activity = state.scan_activity.clone();
        tokio::task::spawn_blocking(move || {
            let scan_guard = (kind == "scan").then(|| scan_activity.start());
            let outcome = if cancellation.load(Ordering::Acquire) {
                Ok(JobOutput {
                    exit_code: 130,
                    ..Default::default()
                })
            } else {
                run(request, cancellation)
            };
            (outcome, scan_guard)
        })
        .await
        .unwrap_or_else(|_| (Err("administrative command worker failed".into()), None))
    };
    // The command has stopped, but its changed catalog is not visible until
    // the server has refreshed the snapshot it publishes to readers.
    let _ = state.events.send(CatalogEvent::TaskProgress {
        task_id,
        task_kind: kind,
        phase: "refreshing",
        done: 0,
        total: 0,
    });
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

// The registry is already capped; reconnecting clients must not guess another process's IDs.
async fn task_list_route(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<Vec<Task>>, ApiError> {
    administrative_body(&state, request, 0).await?;
    let data = state.jobs.data.lock().map_err(|_| registry_error())?;
    Ok(Json(data.tasks.values().cloned().collect()))
}

async fn task_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Json<Task>, ApiError> {
    administrative_body(&state, request, 0).await?;
    Ok(Json(state.jobs.get(task_id(&id)?)?))
}

async fn cancel_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, ApiError> {
    administrative_body(&state, request, 0).await?;
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
    state.audio_features.stop();
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

pub(super) fn progress(state: &ApiState, id: u64, value: NativeProgress) -> Result<(), String> {
    let mut data = state
        .jobs
        .data
        .lock()
        .map_err(|_| "feature task state unavailable")?;
    let task = data.tasks.get_mut(&id).ok_or("feature task disappeared")?;
    task.progress = Some(value.clone());
    drop(data);
    let _ = state.events.send(CatalogEvent::TaskProgress {
        task_id: id,
        task_kind: "mood_features",
        phase: value.phase,
        done: value.done,
        total: value.total,
    });
    Ok(())
}

pub(super) fn enqueue_background(
    state: ApiState,
    run: impl FnOnce(ApiState, Arc<AtomicBool>, u64) -> Result<JobOutput, String> + Send + 'static,
) -> Result<Response, ApiError> {
    if state.admin.is_none() {
        return Err(registry_error());
    }
    let id = state.next_task_id.fetch_add(1, Ordering::Relaxed);
    let cancel = state.jobs.insert(id, "mood_features")?;
    let worker = state.clone();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            worker
                .jobs
                .running(id)
                .map_err(|_| "feature task could not start")?;
            let _ = worker.events.send(CatalogEvent::TaskStarted {
                task_id: id,
                task_kind: "mood_features",
            });
            run(worker, cancel, id)
        })
        .await
        .unwrap_or_else(|_| Err("feature worker stopped".into()));
        match state.jobs.finish(id, result) {
            Ok(task) if task.status == Status::Completed => {
                let _ = state.events.send(CatalogEvent::TaskCompleted {
                    task_id: id,
                    task_kind: "mood_features",
                    scanned_at: None,
                });
            }
            Ok(task) => {
                if let Some(failure) = task.error {
                    let _ = state.events.send(CatalogEvent::TaskFailed {
                        task_id: id,
                        task_kind: "mood_features",
                        code: failure.code,
                        message: "acoustic task stopped; inspect its authenticated result".into(),
                    });
                }
            }
            Err(_) => eprintln!("acoustic task could not publish completion"),
        }
    });
    Ok((
        StatusCode::ACCEPTED,
        Json(Accepted {
            task_id: id,
            status: "queued",
            status_url: format!("/api/admin/v1/tasks/{id}"),
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckSelection {
    release: String,
}
async fn check_route(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    check_request(&state, &request)?;
    let principal = request
        .extensions()
        .get::<super::auth::Principal>()
        .cloned();
    let selected: CheckSelection = decode(&body(request, MAX_BODY).await?)?;
    if selected.release.len() > super::MAX_REFERENCE_BYTES {
        return Err(super::invalid_query("release reference is too long"));
    }
    let files = super::inspection::inspect(state.clone(), move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(super::unavailable)?;
        let id = super::catalog::filter_reference(
            catalog,
            &selected.release,
            aede_core::model::EntityKind::Release,
        )?;
        let release = catalog.release(id).ok_or_else(super::unavailable)?;
        let mut seen = std::collections::BTreeSet::new();
        let mut files = Vec::new();
        for id in &release.track_ids {
            let track = catalog.track(*id).ok_or_else(super::unavailable)?;
            let file = catalog.file(track.file_id).ok_or_else(super::unavailable)?;
            if seen.insert(file.path.clone()) {
                files.push(file.path.clone());
            }
        }
        if let Some(principal) = &principal {
            super::auth::recheck(state, principal)?;
        }
        Ok(files)
    })
    .await?
    .0;
    enqueue(state, JobRequest::Check(files.into()))
}

#[cfg(test)]
#[path = "job_session_tests.rs"]
mod session_tests;
