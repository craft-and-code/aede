//! Explicit original-audio exports reuse the copy plan and revocable media transport.
use crate::*;
use aede_core::copy::{self, Extras, Recipe};
use axum::http::HeaderMap;
use serde_json::{Value, json};

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/api/me/v1/exports/plan", post(plan))
        .route("/api/me/v1/exports/original", get(original))
}

/// Raw originals cannot enforce supervised playback schedules after export.
pub(super) fn admit(state: &ApiState, principal: &auth::Principal) -> Result<(), ApiError> {
    auth::recheck(state, principal)?;
    auth::require_mutation(principal)?;
    if parental::snapshot(state, &principal.owner)?.policy.enabled {
        return Err(parental::refused("artist_blocked"));
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    references: Vec<String>,
    format: Option<String>,
}

fn track_reference(token: &str) -> Result<EntityRef, ApiError> {
    EntityRef::parse_token(token)
        .filter(|value| value.kind == EntityKind::Track && token.len() <= MAX_REFERENCE_BYTES)
        .ok_or_else(|| invalid_query("a bounded track reference is required"))
}

async fn plan(State(state): State<ApiState>, request: Request) -> Result<Json<Value>, ApiError> {
    personal::no_query(&request)?;
    let principal = auth::request_principal(&request)?;
    let selection: Selection = personal::json_body(request).await?;
    if selection.references.is_empty() || selection.references.len() > 200 {
        return Err(invalid_query(
            "select between 1 and 200 tracks per export plan",
        ));
    }
    let target = match selection.format.as_deref() {
        None | Some("original") => None,
        Some(value) => Some(
            copy::transcode::Target::parse(value)
                .ok_or_else(|| invalid_query("unsupported export conversion format"))?,
        ),
    };
    let to_read = state.clone();
    let result = parental::blocking(&state, move || {
        admit(&to_read, &principal)?;
        let held = to_read.catalog.blocking_read();
        let catalog = held.as_ref().ok_or_else(unavailable)?;
        let mut tracks = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for token in &selection.references {
            let reference = track_reference(token)?;
            let id = reference.resolve(catalog).ok_or_else(unavailable)?;
            if !seen.insert(id) { return Err(invalid_query("duplicate track reference")); }
            tracks.push(id);
        }
        let recipe = Recipe { extras: Extras::None, restrict_names: true, convert: None, quality: None };
        let plan = copy::plan(catalog, &tracks, &recipe);
        if !plan.rejected.is_empty() || !plan.rootless.is_empty() {
            return Err(unavailable());
        }
        let mut rows = Vec::new();
        for token in selection.references {
            let reference = track_reference(&token)?;
            let source = playback_api::source_from_catalog(catalog, &reference).map_err(|_| unavailable())?;
            let item = plan.items.iter().find(|item| item.source == source.path).ok_or_else(unavailable)?;
            copy::validate_source(&source.path).map_err(|_| unavailable())?;
            let components: Vec<_> = item.relative.components().map(|part| part.as_os_str().to_str().map(str::to_owned).ok_or_else(unavailable)).collect::<Result<_, _>>()?;
            let key: Vec<_> = components.iter().map(|part| copy::names::portable_key(part)).collect();
            let output = copy::staged_output_path(&source.file, &components.join("/"), target);
            let output_components: Vec<_> = output.components().map(|part| part.as_os_str().to_str().map(str::to_owned).ok_or_else(unavailable)).collect::<Result<_, _>>()?;
            let output_key: Vec<_> = output_components.iter().map(|part| copy::names::portable_key(part)).collect();
            rows.push(json!({"reference":token,"components":components,"key":key,"output_components":output_components,"output_key":output_key,"bytes":source.file.size,"stamp":crate::subsonic::media::source_stamp(&source)}));
        }
        admit(&to_read, &principal)?;
        Ok(json!({"version":1,"scanned_at":catalog.scanned_at,"items":rows,"roots":catalog.roots}))
    }).await?;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Original {
    stamp: String,
    #[serde(rename = "ref")]
    reference: String,
}
async fn original(State(state): State<ApiState>, request: Request) -> Result<Response, ApiError> {
    let principal = auth::request_principal(&request)?;
    let Query(query): Query<Original> = Query::try_from_uri(request.uri())
        .map_err(|_| invalid_query("invalid original reference"))?;
    let reference = track_reference(&query.reference)?;
    let headers: HeaderMap = request.headers().clone();
    let to_check = state.clone();
    let captured = principal.clone();
    parental::blocking(&state, move || admit(&to_check, &captured)).await?;
    subsonic::media::response_for(
        state,
        subsonic::media::MediaIdentity::Session(principal),
        reference,
        headers,
        true,
        Some(query.stamp),
    )
    .await
    .map_err(|_| {
        error(
            StatusCode::CONFLICT,
            "export_unavailable",
            "original audio is unavailable or authorization changed; refresh and retry",
        )
    })
}

#[cfg(test)]
#[path = "exports_tests.rs"]
mod tests;
