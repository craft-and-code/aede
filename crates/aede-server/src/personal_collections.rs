//! Atomic name-only collection edits; the saved expression and creation date are retained.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    name: String,
}
pub(super) async fn rename(
    State(state): State<ApiState>,
    input: Result<Query<NameQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<CollectionView>, ApiError> {
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let query = parsed_query(input)?;
    validate_collection_name(&query.name)?;
    let edit: Rename = json_body(request).await?;
    validate_collection_name(&edit.name)?;
    let data_dir = state.data_dir.clone();
    spawn_personal(state.inspection_slots.clone(), move || {
        locked_update(&data_dir, &state, &owner, |_, data| {
            let old = data
                .collection(&owner.id, &query.name)
                .ok_or_else(|| {
                    error(
                        StatusCode::NOT_FOUND,
                        "collection_not_found",
                        "no collection has this name",
                    )
                })?
                .name
                .clone();
            if data
                .collection(&owner.id, &edit.name)
                .is_some_and(|row| row.name != old)
            {
                return Err(error(
                    StatusCode::CONFLICT,
                    "collection_exists",
                    "a collection already has this name",
                ));
            }
            let row = data
                .collections
                .iter_mut()
                .find(|row| row.owner == owner.id && row.name == old)
                .ok_or_else(unavailable)?;
            row.name = edit.name.trim().to_owned();
            row.updated_at = clock::now_seconds();
            Ok(Json(collection_view(row)))
        })
    })?
    .await
    .map_err(|_| unavailable())?
}
