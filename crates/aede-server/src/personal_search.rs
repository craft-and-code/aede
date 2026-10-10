//! Owner-scoped advanced search reuses the canonical saved-query evaluator.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    expression: String,
    offset: Option<usize>,
    limit: Option<usize>,
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/search", get(search))
}
async fn search(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let input = parsed_query(input)?;
    if input.expression.len() > 8192 {
        return Err(invalid_query("search exceeds 8192 bytes"));
    }
    let (offset, limit) = page_bounds(
        input.offset.map(|v| v.to_string()).as_deref(),
        input.limit.map(|v| v.to_string()).as_deref(),
    )?;
    let parsed =
        aede_core::query::parse(&input.expression).map_err(|e| invalid_query(e.to_string()))?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&data_dir)?;
        owner.recheck(&state)?;
        let sources = inspection::held_sources(&data_dir)?;
        let context =
            aede_core::query::Context::new(&catalog, &data, &owner.id).with_sources(&sources);
        let rows = aede_core::query::run(&parsed, &context);
        Ok(Json(crate::presentation::track_pages::render_rows(
            &catalog, &rows, offset, limit,
        )?))
    })?
    .await
    .map_err(|_| unavailable())?
}

#[cfg(test)]
#[path = "personal_search_tests.rs"]
mod tests;
