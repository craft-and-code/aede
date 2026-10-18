//! HTTP route registration.

use super::*;

pub(super) fn router(state: ApiState, address: SocketAddr) -> Router {
    router_with_policy(state, OriginPolicy::Local(address))
}

/// HTTPS administration requires a named administrator session, never a legacy token.
pub(super) fn remote_router(mut state: ApiState, authority: RemoteAuthority) -> Router {
    if let Some(admin) = state.admin.as_mut() {
        admin.token.clear();
    }
    router_with_policy(state, OriginPolicy::Remote(authority))
}

fn application_routes(admin_enabled: bool) -> Router<ApiState> {
    let mut routes = Router::new()
        .route("/", get(landing::page))
        .route("/api/v1/status", get(status))
        .route("/api/v1/library", get(library))
        .route("/api/v1/artists", get(artists))
        .route("/api/v1/releases", get(releases))
        .route("/api/v1/tracks", get(tracks))
        .route("/api/v1/recordings", get(recordings))
        .route("/api/v1/entities", get(entity))
        .route("/api/v1/events", get(events))
        .route("/api/v1/activity", get(activity))
        .merge(accounts_api::routes())
        .merge(personal::account_routes())
        .merge(playback_api::routes())
        .merge(crate::parental::http::routes())
        .merge(subsonic::routes())
        .merge(catalog_commands::routes())
        .merge(inspection::routes())
        .merge(lyrics::routes())
        .merge(waveform::routes())
        .merge(audio_features::routes())
        .merge(presentation::routes())
        .merge(artwork::routes())
        .merge(analysis_report::routes());
    if admin_enabled {
        routes = routes
            .route("/api/admin/v1/scan", post(jobs::scan_route))
            .merge(jobs::routes())
            .merge(personal::routes());
    }
    routes
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
}

fn router_with_policy(state: ApiState, policy: OriginPolicy) -> Router {
    use tower::ServiceExt;
    let remote = matches!(policy, OriginPolicy::Remote(_));
    let access_log = access_log::AccessLog::from_environment();
    let mut template = application_routes(state.admin.is_some());
    if access_log.is_some() {
        template = template.layer(middleware::from_fn(access_log::label));
    }
    let mut routes =
        Router::new().fallback(move |State(state): State<ApiState>, request: Request| {
            let template = template.clone();
            async move {
                let principal = request.extensions().get::<auth::Principal>().cloned();
                let scoped = match principal.clone() {
                    Some(owner)
                        if !request.uri().path().starts_with("/api/auth/")
                            && !request.uri().path().starts_with("/api/admin/")
                            && !request.uri().path().starts_with("/api/me/v1/parental") =>
                    {
                        crate::parental::scoped(
                            state,
                            owner.owner,
                            owner.role == aede_core::accounts::Role::Administrator,
                            request.uri().path() != "/api/me/v1/playback",
                        )
                        .await
                    }
                    _ => Ok(state),
                };
                let scoped = match scoped {
                    Ok(state) => state,
                    Err(error) => return error.into_response(),
                };
                if let Err(error) = crate::parental::route_allowed(&scoped, request.uri().path()) {
                    return error.into_response();
                }
                let response = match template.with_state(scoped.clone()).oneshot(request).await {
                    Ok(response) => response,
                    Err(never) => match never {},
                };
                if scoped.parental_scope.is_some() {
                    let for_check = scoped.clone();
                    if let Err(error) = crate::parental::blocking(&scoped, move || {
                        if let Some(principal) = &principal {
                            auth::recheck(&for_check, principal)?;
                        }
                        crate::parental::finish(&for_check)
                    })
                    .await
                    {
                        return error.into_response();
                    }
                }
                response
            }
        });
    if remote {
        routes = routes.layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_remote_request_budget,
        ));
    }
    let routes = routes
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::enforce_accounts,
        ))
        .layer(middleware::from_fn_with_state(policy, enforce_origin))
        .with_state(state);
    match access_log {
        Some(log) => routes.layer(middleware::from_fn_with_state(log, access_log::record)),
        None => routes,
    }
}
