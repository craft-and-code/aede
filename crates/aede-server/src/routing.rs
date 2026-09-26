//! HTTP route registration.

use super::*;

pub(super) fn router(state: ApiState, address: SocketAddr) -> Router {
    let admin_enabled = state.admin.is_some();
    let mut routes = Router::new()
        .route("/api/v1/status", get(status))
        .route("/api/v1/library", get(library))
        .route("/api/v1/artists", get(artists))
        .route("/api/v1/releases", get(releases))
        .route("/api/v1/tracks", get(tracks))
        .route("/api/v1/recordings", get(recordings))
        .route("/api/v1/entities", get(entity))
        .route("/api/v1/events", get(events))
        .route("/api/v1/activity", get(activity))
        .merge(catalog_commands::routes())
        .merge(inspection::routes());
    if admin_enabled {
        routes = routes
            .route("/api/admin/v1/scan", post(jobs::scan_route))
            .merge(jobs::routes());
    }
    routes
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(
            address,
            enforce_local_origin,
        ))
        .with_state(state)
}
