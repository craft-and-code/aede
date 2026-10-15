//! A bounded request-state shadow keeps graph filtering ahead of all projections.
use super::*;

struct Cached {
    owner: String,
    revision: u64,
    publication: u64,
    blocked: Vec<String>,
    catalog: Arc<RwLock<Option<Catalog>>>,
}
pub(crate) struct ViewState {
    pub(super) publication: AtomicU64,
    pub(crate) seen_store: std::sync::atomic::AtomicBool,
    pub(crate) read_slots: Arc<Semaphore>,
    cached: BlockingMutex<Vec<Cached>>,
    building: BlockingMutex<()>,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            publication: AtomicU64::new(0),
            seen_store: std::sync::atomic::AtomicBool::new(false),
            read_slots: Arc::new(Semaphore::new(8)),
            cached: BlockingMutex::new(Vec::new()),
            building: BlockingMutex::new(()),
        }
    }
}
impl ViewState {
    pub(crate) fn invalidate(&self) {
        self.publication.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut cached) = self.cached.lock() {
            cached.clear();
        }
    }
}

fn view(
    state: &ApiState,
    full: &Catalog,
    scope: &Scope,
) -> Result<Arc<RwLock<Option<Catalog>>>, ApiError> {
    // A cache miss must not multiply full graph allocations across concurrent requests.
    let _building = state.parental.building.lock().map_err(|_| unavailable())?;
    let publication = state.parental.publication.load(Ordering::Acquire);
    if let Some(cached) = state
        .parental
        .cached
        .lock()
        .map_err(|_| unavailable())?
        .iter()
        .find(|entry| {
            entry.owner == scope.owner
                && entry.revision == scope.revision
                && entry.publication == publication
                && entry.blocked == scope.policy.blocked_artists
        })
        .map(|entry| entry.catalog.clone())
    {
        return Ok(cached);
    }
    let visible = aede_core::parental::filtered_catalog(full, &scope.policy.blocked_artists)
        .map_err(|_| unavailable())?;
    if publication != state.parental.publication.load(Ordering::Acquire) {
        return Err(unavailable());
    }
    let visible = Arc::new(RwLock::new(Some(visible)));
    let mut cached = state.parental.cached.lock().map_err(|_| unavailable())?;
    cached.retain(|entry| entry.owner != scope.owner && entry.publication == publication);
    if cached.len() >= 2 {
        cached.remove(0);
    }
    cached.push(Cached {
        owner: scope.owner.clone(),
        revision: scope.revision,
        publication,
        blocked: scope.policy.blocked_artists.clone(),
        catalog: visible.clone(),
    });
    Ok(visible)
}

/// Reuse the bounded projection at each PCM source transition without retaining it in a socket.
pub(crate) fn visible(
    state: &ApiState,
    full: &Catalog,
) -> Result<Option<Arc<RwLock<Option<Catalog>>>>, ApiError> {
    confirm(state)?;
    match state
        .parental_scope
        .as_ref()
        .filter(|scope| scope.policy.enabled && !scope.policy.blocked_artists.is_empty())
    {
        Some(scope) => view(state, full, scope).map(Some),
        None => Ok(None),
    }
}

pub(crate) async fn scoped(
    mut state: ApiState,
    owner: String,
    admin: bool,
    project: bool,
) -> Result<ApiState, ApiError> {
    if admin {
        return Ok(state);
    }
    let for_read = state.clone();
    let to_read = owner.clone();
    let snapshot = blocking(&state, move || snapshot(&for_read, &to_read)).await?;
    let publication = state.parental.publication.load(Ordering::Acquire);
    let scope = Scope {
        owner,
        revision: snapshot.revision,
        policy: snapshot.policy,
        publication,
    };
    state.parental_scope = Some(scope);
    if project && supervised(&state) {
        let for_work = state.clone();
        let visible = blocking(&state, move || {
            let input = for_work.catalog.blocking_read();
            visible(&for_work, input.as_ref().ok_or_else(unavailable)?)
        })
        .await?;
        if let Some(visible) = visible {
            state.catalog = visible;
        }
    }
    Ok(state)
}

pub(crate) fn finish(state: &ApiState) -> Result<(), ApiError> {
    confirm(state)?;
    if state.parental_scope.as_ref().is_some_and(|scope| {
        scope.publication != state.parental.publication.load(Ordering::Acquire)
    }) {
        return Err(error(
            StatusCode::CONFLICT,
            "catalog_changed",
            "the library changed; refresh the view",
        ));
    }
    Ok(())
}

pub(crate) fn route_allowed(state: &ApiState, path: &str) -> Result<(), ApiError> {
    if supervised(state)
        && (path.starts_with("/api/me/v1/m3u-files")
            || matches!(
                path,
                "/api/v1/roots" | "/api/v1/doctor" | "/api/v1/events" | "/api/v1/activity"
            )
            || (path.starts_with("/api/me/v1/m3u/") && !path.ends_with("/playback")))
    {
        return Err(refused("artist_blocked"));
    }
    Ok(())
}
