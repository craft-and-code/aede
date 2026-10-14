//! Server-owned cooperative extraction and shared acoustic facts; personal models remain request-local.
use super::*;
use aede_core::audio_features::{InputIdentity, Record};
use aede_core::user::EntityRef;
use axum::routing::post;
use std::collections::BTreeMap;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
mod projection;
mod worker;
pub(super) use projection::{Window, projection};
const MAX_FILES: usize = 16384;
#[derive(Default)]
pub(super) struct Facts {
    pub(super) rows: BTreeMap<String, Record>,
    pub(super) epoch: Option<u64>,
    pub(super) cache_errors: usize,
    pub(super) revision: u64,
}
#[derive(Default)]
pub(crate) struct FeatureState {
    facts: StdMutex<Facts>,
    worker: StdMutex<()>,
    warming: AtomicBool,
    stopping: AtomicBool,
    leases: StdMutex<BTreeMap<(String, String), Instant>>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Parameters {
    force: bool,
}
pub(super) async fn start(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Response, ApiError> {
    jobs::check_request(&state, &request)?;
    let principal = request.extensions().get::<auth::Principal>().cloned();
    let input: Parameters = jobs::decode(&jobs::body(request, 256).await?)?;
    let total = state
        .catalog
        .read()
        .await
        .as_ref()
        .ok_or_else(unavailable)?
        .files
        .len();
    if total > MAX_FILES {
        return Err(invalid_query(
            "acoustic profile supports at most 16384 catalogued files",
        ));
    }
    if let Some(principal) = principal {
        auth::recheck(&state, &principal)?;
    }
    jobs::enqueue_background(state, move |state, cancel, id| {
        worker::run(state, cancel, id, input.force)
    })
}
pub(super) fn identity(source: &playback_api::TrackSource) -> InputIdentity {
    InputIdentity {
        path: source.file.path.clone(),
        size: source.file.size,
        modified: source.file.mtime,
        nanos: source.mtime_subseconds,
    }
}
fn sources(state: &ApiState) -> Result<(u64, Vec<playback_api::TrackSource>), String> {
    let guard = state.catalog.blocking_read();
    let catalog = guard.as_ref().ok_or("catalog unavailable")?;
    if catalog.files.len() > MAX_FILES {
        return Err("acoustic catalogue budget exceeded".into());
    }
    Ok((
        signature(catalog),
        catalog
            .files
            .iter()
            .map(|file| playback_api::TrackSource {
                reference: EntityRef::new(EntityKind::Track, file.path.clone()),
                path: PathBuf::from(&file.path),
                file: file.clone(),
                mtime_subseconds: catalog
                    .file_mtime_subseconds
                    .get(&file.path)
                    .copied()
                    .unwrap_or(0),
            })
            .collect(),
    ))
}
pub(super) fn warm(state: &ApiState) {
    let epoch = state
        .catalog
        .try_read()
        .ok()
        .and_then(|c| c.as_ref().map(signature));
    let loaded = state
        .audio_features
        .facts
        .lock()
        .ok()
        .and_then(|facts| facts.epoch);
    if epoch.is_none()
        || loaded == epoch
        || state.audio_features.warming.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let result: Result<(), String> = (|| {
            let (epoch, sources) = sources(&state)?;
            let mut shutdown = state.shutdown.subscribe();
            let cancel = AtomicBool::new(false);
            let mut control =
                worker::Control::new(&state, &cancel, &mut shutdown, None, Default::default());
            let _worker = worker::lock(&state, || control.checkpoint())?;
            let mut rows = BTreeMap::new();
            let mut errors = 0;
            for source in sources {
                control.checkpoint()?;
                match aede_core::audio_features::load(&state.data_dir, &identity(&source)) {
                    Ok(Some(record)) => {
                        rows.insert(record.input.path.clone(), record);
                    }
                    Ok(None) => {}
                    Err(_) => errors += 1,
                }
            }
            if state
                .catalog
                .blocking_read()
                .as_ref()
                .is_none_or(|catalog| signature(catalog) != epoch)
            {
                return Err("catalog changed while warming features".to_string());
            }
            let mut facts = state
                .audio_features
                .facts
                .lock()
                .map_err(|_| "feature state unavailable")?;
            if facts.rows != rows {
                facts.revision = facts.revision.saturating_add(1);
            }
            facts.rows = rows;
            facts.epoch = Some(epoch);
            facts.cache_errors = errors;
            Ok(())
        })();
        if let Err(message) = result {
            eprintln!("acoustic cache warming: {message}");
        }
        state.audio_features.warming.store(false, Ordering::Release);
    });
}
pub(super) fn facts(state: &ApiState) -> Result<std::sync::MutexGuard<'_, Facts>, ApiError> {
    state.audio_features.facts.lock().map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "features_unavailable",
            "acoustic state unavailable",
        )
    })
}
pub(super) fn current<'a>(
    facts: &'a Facts,
    catalog: &Catalog,
    file_id: Id,
) -> Option<&'a aede_moods::Features> {
    let file = catalog.file(file_id)?;
    let record = facts.rows.get(&file.path)?;
    (record.input.size == file.size
        && record.input.modified == file.mtime
        && record.input.nanos
            == catalog
                .file_mtime_subseconds
                .get(&file.path)
                .copied()
                .unwrap_or(0))
    .then_some(&record.features)
}
pub(super) fn coverage(state: &ApiState, catalog: &Catalog, facts: &Facts) -> serde_json::Value {
    let ready = catalog
        .files
        .iter()
        .filter(|f| current(facts, catalog, f.id).is_some())
        .count();
    serde_json::json!({"profile":aede_moods::PROFILE,"ready":ready,"missing":catalog.files.len()-ready,"cache_errors":facts.cache_errors,"warming":state.audio_features.warming.load(Ordering::Acquire)})
}
#[cfg(test)]
#[path = "audio_features_tests.rs"]
mod tests;

impl FeatureState {
    pub(super) fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Priority {
    client: String,
    active: bool,
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/background-priority", post(priority))
}
async fn priority(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    personal::no_query(&request)?;
    let principal = auth::request_principal(&request)?;
    let data: Priority = jobs::decode(&jobs::body(request, 256).await?)?;
    if data.client.len() != 36
        || !data.client.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
    {
        return Err(invalid_query("client must be a UUID"));
    }
    auth::recheck(&state, &principal)?;
    let mut leases = state
        .audio_features
        .leases
        .lock()
        .map_err(|_| unavailable())?;
    leases.retain(|_, time| time.elapsed() < Duration::from_secs(6));
    let key = (principal.owner.clone(), data.client);
    if data.active {
        if !leases.contains_key(&key)
            && (leases.len() >= 64
                || leases
                    .keys()
                    .filter(|(owner, _)| owner == &principal.owner)
                    .count()
                    >= 8)
        {
            return Err(error(
                StatusCode::TOO_MANY_REQUESTS,
                "priority_limit",
                "foreground lease capacity reached",
            ));
        }
        leases.insert(key, Instant::now());
    } else {
        leases.remove(&key);
    }
    Ok(Json(
        serde_json::json!({"active":data.active,"expires_in_ms":6000}),
    ))
}

fn signature(catalog: &Catalog) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    catalog.scanned_at.hash(&mut hash);
    for file in &catalog.files {
        (
            file.path.as_str(),
            file.size,
            file.mtime,
            catalog.file_mtime_subseconds.get(&file.path),
        )
            .hash(&mut hash);
    }
    hash.finish()
}
