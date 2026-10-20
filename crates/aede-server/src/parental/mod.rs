//! Server-owned parental admission, independent of editable personal preferences.
use crate::*;
use aede_core::parental::{Media, ParentalStore, PolicySnapshot};
use std::sync::Mutex as BlockingMutex;

mod catalog;
pub(super) mod http;
pub(super) mod pcm;
pub(super) use catalog::{ViewState, scoped};
pub(super) use catalog::{finish, route_allowed, visible};

#[derive(Clone)]
pub(super) struct Scope {
    pub(super) owner: String,
    pub(super) revision: u64,
    pub(super) policy: aede_core::parental::Policy,
    publication: u64,
    // Shared original-audio facts use this graph, never the account projection.
    // Account-visible responses must continue to use ApiState::catalog.
    pub(super) shared_catalog: Arc<RwLock<Option<Catalog>>>,
}

pub(super) fn unavailable() -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "parental_policy_unavailable",
        "parental policy could not be verified",
    )
}
pub(super) fn refused(reason: &str) -> ApiError {
    let code = match reason {
        "outside_schedule" => "parental_outside_schedule",
        "daily_limit" => "parental_daily_limit",
        "weekly_limit" => "parental_weekly_limit",
        "radio_blocked" => "parental_radio_blocked",
        "podcast_blocked" => "parental_podcast_blocked",
        "artist_blocked" => "parental_artist_blocked",
        _ => "parental_policy_unavailable",
    };
    error(
        StatusCode::FORBIDDEN,
        code,
        "playback is restricted by this account's parental policy",
    )
}
pub(super) fn load(state: &ApiState) -> Result<ParentalStore, ApiError> {
    match ParentalStore::load_optional(&aede_core::parental::path(&state.data_dir)) {
        Ok(Some(data)) => {
            state.parental.seen_store.store(true, Ordering::Release);
            Ok(data)
        }
        Ok(None) if !state.parental.seen_store.load(Ordering::Acquire) => {
            Ok(ParentalStore::default())
        }
        Ok(None) | Err(_) => {
            state.parental.seen_store.store(true, Ordering::Release);
            Err(unavailable())
        }
    }
}
pub(super) fn snapshot(state: &ApiState, owner: &str) -> Result<PolicySnapshot, ApiError> {
    Ok(load(state)?.policy(owner))
}
pub(super) fn save(state: &ApiState, data: &ParentalStore) -> Result<(), ApiError> {
    data.save(&aede_core::parental::path(&state.data_dir))
        .map_err(|_| unavailable())?;
    state.parental.seen_store.store(true, Ordering::Release);
    Ok(())
}
pub(super) fn lock(state: &ApiState) -> Result<StoreLock, ApiError> {
    StoreLock::try_acquire(&state.data_dir).map_err(|_| {
        error(
            StatusCode::CONFLICT,
            "store_busy",
            "Aède data is busy; retry shortly",
        )
    })
}
pub(super) async fn blocking<T: Send + 'static>(
    state: &ApiState,
    work: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let permit = state
        .parental
        .read_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| unavailable())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|_| unavailable())?
}
pub(super) fn confirm(state: &ApiState) -> Result<(), ApiError> {
    if let Some(scope) = &state.parental_scope {
        let current = snapshot(state, &scope.owner)?;
        if current.revision != scope.revision || current.policy != scope.policy {
            return Err(error(
                StatusCode::CONFLICT,
                "parental_conflict",
                "parental policy changed; refresh the library",
            ));
        }
    }
    Ok(())
}
pub(super) fn view_owned(state: &ApiState, catalog: Catalog) -> Result<Catalog, ApiError> {
    confirm(state)?;
    match &state.parental_scope {
        Some(scope) if scope.policy.enabled && !scope.policy.blocked_artists.is_empty() => {
            aede_core::parental::filtered_catalog(&catalog, &scope.policy.blocked_artists)
                .map_err(|_| unavailable())
        }
        _ => Ok(catalog),
    }
}
pub(super) fn supervised(state: &ApiState) -> bool {
    state
        .parental_scope
        .as_ref()
        .is_some_and(|scope| scope.policy.enabled)
}
pub(super) fn media_status(
    state: &ApiState,
    owner: &str,
    media: &Media,
) -> Result<aede_core::parental::Status, ApiError> {
    load(state)?
        .status(owner, aede_core::clock::now_seconds(), media)
        .map_err(|_| unavailable())
}
