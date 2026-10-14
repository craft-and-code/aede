//! Bounded, owner-private album completion; public catalog caches never carry this state.
use super::*;
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/album-listening", get(read))
}
async fn read(
    State(state): State<ApiState>,
    input: Result<Query<PageQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<serde_json::Value>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    empty_body(&body(request).await?)?;
    let (offset, limit) = page(parsed_query(input)?)?;
    spawn_personal(state.inspection_slots.clone(), move || {
        let (_guard, catalog, data) = locked_personal(&state.data_dir)?;
        owner.recheck(&state)?;
        let completed = aede_core::listening::completed_albums(&catalog, &data, &owner.id);
        let items: Vec<_> = catalog.releases.iter().skip(offset).take(limit).filter_map(|release| {
            EntityRef::of(&catalog, aede_core::model::EntityKind::Release, release.id).map(|reference| serde_json::json!({"reference":reference.to_token(),"completed":completed.contains(&release.id)}))
        }).collect();
        owner.admit(&state, Json(serde_json::json!({"total":catalog.releases.len(),"offset":offset,"items":items})))
    })?.await.map_err(|_| unavailable())?
}
#[cfg(test)]
#[path = "personal_listening_tests.rs"]
mod tests;
