//! Read-only report selection reconstructs original FlacCompagnon entries.
use super::*;
use aede_core::analysis::export::{self, ExportError};

pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/v1/analysis/report", get(read))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    #[serde(rename = "ref")]
    reference: String,
}
fn export_failure(failure: ExportError) -> ApiError {
    let (status, code, message) = match failure {
        ExportError::InvalidSelection => (
            StatusCode::NOT_FOUND,
            "entity_not_found",
            "no selected track or album is available",
        ),
        ExportError::MissingAnalysis => (
            StatusCode::NOT_FOUND,
            "analysis_unavailable",
            "not every selected file has a retained FlacCompagnon analysis",
        ),
        ExportError::StaleAnalysis => (
            StatusCode::CONFLICT,
            "analysis_stale",
            "a selected analysis no longer describes the catalogued file",
        ),
        ExportError::IncompleteAnalysis => (
            StatusCode::CONFLICT,
            "analysis_incomplete",
            "complete source entries are unavailable; import a newer report or analyze again",
        ),
        ExportError::UnsupportedVersion => (
            StatusCode::CONFLICT,
            "analysis_version",
            "a selected report requires a newer Aede version",
        ),
        ExportError::TooLarge => (
            StatusCode::PAYLOAD_TOO_LARGE,
            "analysis_too_large",
            "selected report exceeds 2048 tracks or 16 MiB",
        ),
    };
    error(status, code, message)
}
async fn read(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Response, ApiError> {
    let Query(parameters) = input.map_err(|failure| invalid_query(failure.body_text()))?;
    if parameters.reference.len() > MAX_REFERENCE_BYTES {
        return Err(invalid_query("report reference is too long"));
    }
    let entity = EntityRef::parse_token(&parameters.reference)
        .filter(|entity| matches!(entity.kind, EntityKind::Track | EntityKind::Release))
        .ok_or_else(|| invalid_query("report needs a track or release reference"))?;
    let filename = if entity.kind == EntityKind::Track {
        "flaccompagnon-track.json"
    } else {
        "flaccompagnon-album.json"
    };
    let principal = request.extensions().get::<auth::Principal>().cloned();
    personal::empty_body(&personal::body(request).await?)?;
    let content = inspection::inspect(state, move |state| {
        let guard = state.catalog.blocking_read();
        let catalog = guard.as_ref().ok_or_else(unavailable)?;
        let id = entity
            .resolve(catalog)
            .ok_or_else(|| export_failure(ExportError::InvalidSelection))?;
        let content = export::report(catalog, entity.kind, id).map_err(export_failure)?;
        if let Some(principal) = principal.as_ref() {
            auth::recheck(state, principal)?;
        }
        Ok(content)
    })
    .await?
    .0;
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(axum::body::Body::from(content))
        .map_err(|_| unavailable())
}
#[cfg(test)]
#[path = "analysis_report_tests.rs"]
mod tests;
