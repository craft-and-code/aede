//! Administrative parental policy editing under the account writer lock.
use super::*;
use aede_core::parental::{Policy, PolicySnapshot};

pub(super) fn routes() -> Router<ApiState> {
    Router::new().route(
        "/api/admin/v1/accounts/:username/parental",
        get(read).put(update),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    expected_owner: String,
    expected_revision: u64,
    policy: Policy,
}

fn target<'a>(accounts: &'a Accounts, name: &str) -> Result<&'a Account, ApiError> {
    let account = accounts.find(name).ok_or_else(missing)?;
    if account.role == Role::Administrator {
        return Err(refused(
            "parental policies cannot restrict an administrator",
        ));
    }
    Ok(account)
}
async fn read(
    State(state): State<ApiState>,
    input: Result<PathParameter<String>, PathRejection>,
    request: Request,
) -> Result<Json<PolicySnapshot>, ApiError> {
    let captured = admin_capture(&state, &request)?;
    personal::empty_body(&personal::body(request).await?)?;
    let name = path_name(input)?;
    let for_work = state.clone();
    crate::parental::blocking(&state, move || {
        let _guard = lock_accounts(&for_work)?;
        check_capture(&for_work, captured.as_ref())?;
        let accounts = enabled_accounts(&for_work)?;
        let account = target(&accounts, &name)?;
        Ok(Json(crate::parental::snapshot(&for_work, &account.id)?))
    })
    .await
}
async fn update(
    State(state): State<ApiState>,
    input: Result<PathParameter<String>, PathRejection>,
    request: Request,
) -> Result<Json<PolicySnapshot>, ApiError> {
    let captured = admin_capture(&state, &request)?;
    let name = path_name(input)?;
    let mut change: Change = personal::json_body(request).await?;
    change.policy.blocked_podcasts = change
        .policy
        .blocked_podcasts
        .iter()
        .map(|feed| crate::parental::http::canonical_feed(feed))
        .collect::<Result<_, _>>()?;
    let for_work = state.clone();
    crate::parental::blocking(&state, move || {
        let _guard = lock_accounts(&for_work)?;
        check_capture(&for_work, captured.as_ref())?;
        let accounts = enabled_accounts(&for_work)?;
        let account = target(&accounts, &name)?;
        if account.id != change.expected_owner {
            return Err(error(
                StatusCode::CONFLICT,
                "parental_conflict",
                "the target account changed; reload before saving",
            ));
        }
        let mut data = crate::parental::load(&for_work)?;
        let snapshot = data.policy(&account.id);
        let administrator = captured.as_ref().map(|principal| principal.owner.as_str());
        if snapshot
            .locked_by
            .as_deref()
            .is_some_and(|id| Some(id) != administrator)
        {
            return Err(locked());
        }
        if change.policy.enabled && administrator.is_none() {
            return Err(error(
                StatusCode::FORBIDDEN,
                "parental_administrator_required",
                "sign in as an administrator to own the supervision lock",
            ));
        }
        if data.policy(&account.id).revision != change.expected_revision {
            return Err(error(
                StatusCode::CONFLICT,
                "parental_conflict",
                "the parental policy changed; reload before saving",
            ));
        }
        let saved = data
            .set_policy_by(
                &account.id,
                administrator,
                change.policy,
                change.expected_revision,
            )
            .map_err(refused)?;
        check_capture(&for_work, captured.as_ref())?;
        crate::parental::save(&for_work, &data)?;
        for_work.parental.invalidate();
        Ok(Json(saved))
    })
    .await
}

pub(super) fn locked() -> ApiError {
    error(
        StatusCode::FORBIDDEN,
        "parental_locked",
        "this account is locked by its supervising administrator",
    )
}

pub(super) fn check_account_edit(
    state: &ApiState,
    actor: Option<&auth::Principal>,
    account: &Account,
) -> Result<(), ApiError> {
    let data = crate::parental::load(state)?;
    // Another administrator must not reset the supervisor's password and impersonate its lock.
    if data.supervises(&account.id)
        && actor.map(|value| value.owner.as_str()) != Some(account.id.as_str())
    {
        return Err(locked());
    }
    if data
        .policy(&account.id)
        .locked_by
        .as_deref()
        .is_some_and(|id| Some(id) != actor.map(|value| value.owner.as_str()))
    {
        return Err(locked());
    }
    Ok(())
}

pub(super) fn check_administrator_retained(
    state: &ApiState,
    account: &Account,
    role: Option<&str>,
    enabled: Option<bool>,
) -> Result<(), ApiError> {
    if role == Some("admin")
        && crate::parental::load(state)?
            .policy(&account.id)
            .policy
            .enabled
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "parental_locked",
            "disable supervision before promoting this account",
        ));
    }
    if (enabled == Some(false) || role.is_some_and(|role| role != "admin"))
        && crate::parental::load(state)?.supervises(&account.id)
    {
        return Err(error(
            StatusCode::FORBIDDEN,
            "parental_locked",
            "release the supervised account locks before disabling or demoting this administrator",
        ));
    }
    Ok(())
}
