//! Private opaque successes; mutations capture and recheck the immutable authenticated owner.
use super::*;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    #[serde(default)]
    advanced_search: bool,
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/achievements", get(read).post(update))
}
async fn read(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    parsed_query(input)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, mut data) = locked_personal(&state.data_dir)?;
        owner.recheck(&state)?;
        let prepared = aede_core::achievements::refresh(
            &catalog,
            &mut data,
            &owner.id,
            false,
            clock::now_seconds(),
        )
        .map(Json)
        .map_err(invalid_query)?;
        owner.admit(&state, prepared)
    })?
    .await
    .map_err(|_| unavailable())?
}
async fn update(
    State(state): State<ApiState>,
    input: Result<Query<Parameters>, QueryRejection>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    parsed_query(input)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let action: Action = json_body(request).await?;
    spawn_personal(state.inspection_slots.clone(), move || {
        locked_update(&state.data_dir, &state, &owner, |catalog, data| {
            aede_core::achievements::refresh(
                catalog,
                data,
                &owner.id,
                action.advanced_search,
                clock::now_seconds(),
            )
            .map(Json)
            .map_err(invalid_query)
        })
    })?
    .await
    .map_err(|_| unavailable())?
}
#[cfg(test)]
#[path = "personal_achievements_tests.rs"]
mod tests;
