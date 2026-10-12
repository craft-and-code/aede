//! Account preferences can be saved before the first catalog scan.
use super::*;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyQuery {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Patch {
    values: BTreeMap<String, serde_json::Value>,
}
#[derive(Serialize)]
struct View {
    values: BTreeMap<String, serde_json::Value>,
}
pub(super) fn routes() -> Router<ApiState> {
    Router::new().route("/api/me/v1/preferences", get(read).patch(write))
}
fn snapshot(data: &UserData, owner: &str) -> Result<View, ApiError> {
    let values = data
        .client_preferences
        .iter()
        .filter(|row| row.owner == owner && row.key.starts_with("phemios."))
        .map(|row| serde_json::from_str(&row.value).map(|value| (row.key.clone(), value)))
        .collect::<Result<_, _>>()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "user_unavailable",
                "invalid preferences",
            )
        })?;
    Ok(View { values })
}
fn locked(state: &ApiState, owner: &PersonalOwner, patch: Option<Patch>) -> Result<View, ApiError> {
    let _guard = StoreLock::try_acquire(&state.data_dir)
        .map_err(|_| error(StatusCode::CONFLICT, "store_busy", "personal store is busy"))?;
    owner.recheck(state)?;
    let path = user::user_path(&state.data_dir);
    let mut data = user::load(&path)
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "user_unavailable",
                "personal store could not be read",
            )
        })?
        .unwrap_or_default();
    if let Some(patch) = patch {
        for (key, value) in patch.values {
            let value = value.to_string();
            if let Some(row) = data
                .client_preferences
                .iter_mut()
                .find(|row| row.owner == owner.id && row.key == key)
            {
                row.value = value;
            } else {
                data.client_preferences.push(user::ClientPreference {
                    owner: owner.id.clone(),
                    key,
                    value,
                });
            }
        }
        if data.client_preferences.len() > 8192
            || data
                .client_preferences
                .iter()
                .filter(|row| row.owner == owner.id)
                .count()
                > 64
        {
            return Err(invalid_query("too many preferences"));
        }
        owner.recheck(state)?;
        user::save(&data, &path).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "preferences could not be saved",
            )
        })?;
    }
    owner.admit(state, snapshot(&data, &owner.id)?)
}
async fn read(
    State(state): State<ApiState>,
    input: Result<Query<EmptyQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<View>, ApiError> {
    parsed_query(input)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    spawn_personal(state.inspection_slots.clone(), move || {
        locked(&state, &owner, None)
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "preferences worker failed",
        )
    })?
    .map(Json)
}
async fn write(
    State(state): State<ApiState>,
    input: Result<Query<EmptyQuery>, QueryRejection>,
    request: Request,
) -> Result<Json<View>, ApiError> {
    parsed_query(input)?;
    let owner = PersonalOwner::from_request(&state, &request)?;
    owner.require_mutation()?;
    let patch: Patch = json_body(request).await?;
    if patch.values.is_empty()
        || patch.values.len() > 32
        || patch.values.iter().any(|(key, value)| {
            !key.starts_with("phemios.") || !user::valid_preference(key, &value.to_string())
        })
    {
        return Err(invalid_query("invalid application preferences"));
    }
    spawn_personal(state.inspection_slots.clone(), move || {
        locked(&state, &owner, Some(patch))
    })?
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "personal_failed",
            "preferences worker failed",
        )
    })?
    .map(Json)
}
#[cfg(test)]
#[path = "personal_preferences_tests.rs"]
mod tests;
