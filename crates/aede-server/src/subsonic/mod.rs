//! Subsonic-shaped translation over the shared graph, never a second catalog.
//!
//! Revocable application keys authenticate ID3 browsing, original audio,
//! sidecar artwork and owner-private annotations, playlists and declarations.
//! Unsupported operations/options fail explicitly; native bearer sessions
//! and the PCM/DSP WebSocket keep their separate contracts.

use super::*;
use axum::extract::Path as RoutePath;
use serde_json::{Value, json};

mod artwork;
pub(crate) mod authentication;
mod catalog;
pub(crate) mod media;
mod now_playing;
mod parameters;
mod personal;
mod protocol;

use parameters::Parameters;
use protocol::{Format, ProtocolError, opaque_id, utc_date};

pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/rest/:method", get(handle).post(handle))
}

async fn handle(
    State(state): State<ApiState>,
    RoutePath(method): RoutePath<String>,
    request: Request,
) -> Response {
    // Axum's GET route also accepts HEAD. Subsonic GET operations can mutate
    // personal data, so implicit HEAD execution must never reach the adapter.
    if request.method() == axum::http::Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let method = method.strip_suffix(".view").unwrap_or(&method).to_string();
    let headers = request.headers().clone();
    let parameters = match parameters::read(request, &method).await {
        Ok(parameters) => parameters,
        Err(failure) => return protocol::response(Format::Xml, Err(failure)),
    };
    let format = match parameters.format() {
        Ok(format) => format,
        Err(failure) => return protocol::response(Format::Xml, Err(failure)),
    };
    let result = execute(state, &method, parameters, headers).await;
    match result {
        Ok(Answer::Protocol(payload)) => protocol::response(format, Ok(payload)),
        Ok(Answer::Binary(response)) => response,
        Err(failure) => protocol::response(format, Err(failure)),
    }
}

enum Answer {
    Protocol(Value),
    Binary(Response),
}

async fn execute(
    state: ApiState,
    method: &str,
    parameters: Parameters,
    headers: HeaderMap,
) -> Result<Answer, ProtocolError> {
    parameters.common(method == "getOpenSubsonicExtensions")?;
    if method == "getOpenSubsonicExtensions" {
        parameters.allowed(&[])?;
        return Ok(Answer::Protocol(json!({"openSubsonicExtensions": [
            {"name": "apiKeyAuthentication", "versions": [1]},
            {"name": "formPost", "versions": [1]}
        ]})));
    }
    let identity = authentication::authenticate(&state, &parameters).await?;
    let payload = match method {
        "ping" => {
            parameters.allowed(&[])?;
            json!({})
        }
        "getLicense" => {
            parameters.allowed(&[])?;
            json!({"license": {"valid": true}})
        }
        "tokenInfo" => {
            parameters.allowed(&[])?;
            json!({"tokenInfo": {"username": identity.username}})
        }
        "getUser" => {
            parameters.allowed(&["username"])?;
            if !parameters
                .required("username")?
                .eq_ignore_ascii_case(&identity.username)
            {
                return Err(ProtocolError::new(50, "only your own user is available"));
            }
            let audio = identity.role != aede_core::accounts::Role::Auditor;
            // Installation administration is not exposed by this adapter.
            json!({"user": {"username": identity.username, "scrobblingEnabled": audio,
                "adminRole": false, "settingsRole": false, "downloadRole": audio,
                "uploadRole": false, "playlistRole": audio, "coverArtRole": true,
                "commentRole": false, "podcastRole": false, "streamRole": audio,
                "jukeboxRole": false, "shareRole": false, "videoConversionRole": false,
                "folder": [1]}})
        }
        "getCoverArt" => {
            return artwork::response(state, identity, parameters)
                .await
                .map(Answer::Binary);
        }
        "getScanStatus" => {
            parameters.allowed(&[])?;
            let running = state.scan_activity.is_running();
            inspect(state.clone(), move |catalog| {
                catalog::scan_status(catalog, running)
            })
            .await?
        }
        "getNowPlaying" => now_playing::list(&state, &identity, &parameters).await?,
        "scrobble" => match parameters.get("submission") {
            Some("false") => now_playing::report(&state, &identity, &parameters).await?,
            None | Some("true") => {
                personal::execute(state.clone(), identity.clone(), method, parameters).await?
            }
            _ => return Err(ProtocolError::new(10, "submission must be true or false")),
        },
        "getStarred2" | "star" | "unstar" | "setRating" | "getPlaylists" | "getPlaylist"
        | "createPlaylist" | "updatePlaylist" | "deletePlaylist" => {
            personal::execute(state.clone(), identity.clone(), method, parameters).await?
        }
        "stream" | "download" => {
            parameters.allowed(&[
                "id",
                "maxBitRate",
                "format",
                "timeOffset",
                "estimateContentLength",
                "converted",
                "videoSize",
            ])?;
            original_only(&parameters)?;
            authentication::require_audio(&identity)?;
            let wanted = parameters.required("id")?.to_string();
            let track = inspect(state.clone(), move |catalog| {
                catalog::track_reference(catalog, &wanted)
            })
            .await?;
            return media::response(state, identity, track, headers, method == "download")
                .await
                .map(Answer::Binary);
        }
        _ => personal::catalog(state.clone(), identity.clone(), method, parameters).await?,
    };
    authentication::recheck(&state, &identity).await?;
    Ok(Answer::Protocol(payload))
}

fn original_only(parameters: &Parameters) -> Result<(), ProtocolError> {
    for name in ["maxBitRate", "timeOffset"] {
        if parameters.get(name).is_some_and(|value| value != "0") {
            return Err(ProtocolError::new(
                0,
                "transcoding and time offsets are not implemented; request the original",
            ));
        }
    }
    if parameters.get("format").is_some_and(|value| value != "raw")
        || parameters.get("videoSize").is_some()
        || parameters
            .get("converted")
            .is_some_and(|value| value != "false")
        || parameters
            .get("estimateContentLength")
            .is_some_and(|value| !matches!(value, "true" | "false"))
    {
        return Err(ProtocolError::new(0, "only original audio is supported"));
    }
    Ok(())
}

async fn inspect<T: Send + 'static>(
    state: ApiState,
    operation: impl FnOnce(&Catalog) -> Result<T, ProtocolError> + Send + 'static,
) -> Result<T, ProtocolError> {
    let permit = state
        .inspection_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ProtocolError::new(0, "catalog work is busy; retry shortly"))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let guard = state.catalog.blocking_read();
        operation(
            guard
                .as_ref()
                .ok_or_else(|| ProtocolError::new(0, "catalog unavailable"))?,
        )
    })
    .await
    .map_err(|_| ProtocolError::new(0, "catalog work failed"))?
}

#[cfg(test)]
#[path = "integration_tests.rs"]
mod tests;

#[cfg(test)]
mod test_support;

#[cfg(test)]
#[path = "client_tests.rs"]
mod client_tests;
