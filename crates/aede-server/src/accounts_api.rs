//! Account administration and explicit bearer-session endpoints.

use aede_core::accounts::{self, Account, Accounts, Role};
use axum::extract::{Path as PathParameter, rejection::PathRejection};

use super::*;

pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route(
            "/api/auth/v1/session",
            post(login).get(session_info).delete(logout),
        )
        .route("/api/auth/v1/password", axum::routing::put(change_password))
        .route(
            "/api/admin/v1/accounts",
            get(list_accounts).post(create_account),
        )
        .route(
            "/api/admin/v1/accounts/:username",
            get(account_info).patch(update_account),
        )
        .route(
            "/api/admin/v1/accounts/:username/sessions",
            axum::routing::delete(revoke_sessions),
        )
}

#[derive(Serialize)]
pub(super) struct AccountView {
    id: String,
    username: String,
    role: &'static str,
    enabled: bool,
    created_at: u64,
    updated_at: u64,
}

fn account_view(account: &Account) -> AccountView {
    AccountView {
        id: account.id.clone(),
        username: account.username.clone(),
        role: account.role.as_str(),
        enabled: account.enabled,
        created_at: account.created_at,
        updated_at: account.updated_at,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Login {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct SessionView {
    token: String,
    token_type: &'static str,
    expires_at: u64,
    account: AccountView,
}

fn refused(message: impl Into<String>) -> ApiError {
    error(StatusCode::BAD_REQUEST, "account_refused", message)
}

fn missing() -> ApiError {
    error(
        StatusCode::NOT_FOUND,
        "account_not_found",
        "no account has this login name",
    )
}

fn enabled_accounts(state: &ApiState) -> Result<Accounts, ApiError> {
    auth::load_accounts(state)?.ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            "accounts_not_configured",
            "initialize accounts with aede accounts init",
        )
    })
}

async fn login(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<SessionView>, ApiError> {
    personal::no_query(&request)?;
    let input: Login = personal::json_body(request).await?;
    let name = accounts::login_name(&input.username).map_err(refused)?;
    if input.password.len() > accounts::MAX_PASSWORD_BYTES {
        return Err(refused("password exceeds 1024 UTF-8 bytes"));
    }
    auth::take_attempt(&state, &name)?;
    let permit = state
        .auth
        .password_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| auth::busy())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let data = enabled_accounts(&state)?;
        let account = data
            .authenticate(&name, &input.password)
            .ok_or_else(|| {
                error(
                    StatusCode::UNAUTHORIZED,
                    "invalid_credentials",
                    "invalid login name or password",
                )
            })?
            .clone();
        // Password work runs outside the writer lock. Recheck its exact
        // generation before publishing a session, so a concurrent reset wins.
        let _guard = lock_accounts(&state)?;
        let current = enabled_accounts(&state)?;
        let account = current
            .session_account(&account.id, data.epoch(), account.revision())
            .ok_or_else(auth::unauthorized)?;
        let (principal, expires_at) = auth::start_session(&state, &current, account)?;
        Ok(Json(SessionView {
            token: auth::session_token(&principal).into(),
            token_type: "Bearer",
            expires_at,
            account: account_view(account),
        }))
    })
    .await
    .map_err(|_| auth::busy())?
}

async fn session_info(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<AccountView>, ApiError> {
    personal::no_query(&request)?;
    let captured = auth::request_principal(&request)?;
    personal::empty_body(&personal::body(request).await?)?;
    let data = auth::current_accounts(&state)
        .await?
        .ok_or_else(auth::unauthorized)?;
    auth::principal(
        &state,
        &data,
        auth::session_token(&captured),
        false,
        Instant::now(),
    )?;
    data.find(&captured.username)
        .map(account_view)
        .map(Json)
        .ok_or_else(auth::unauthorized)
}

async fn logout(State(state): State<ApiState>, request: Request) -> Result<StatusCode, ApiError> {
    personal::no_query(&request)?;
    let captured = auth::request_principal(&request)?;
    personal::empty_body(&personal::body(request).await?)?;
    auth::end_session(&state, &captured)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasswordChange {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<ApiState>,
    request: Request,
) -> Result<StatusCode, ApiError> {
    personal::no_query(&request)?;
    let captured = auth::request_principal(&request)?;
    auth::require_mutation(&captured)?;
    let input: PasswordChange = personal::json_body(request).await?;
    if input.current_password.len() > accounts::MAX_PASSWORD_BYTES {
        return Err(refused("password exceeds 1024 UTF-8 bytes"));
    }
    auth::take_attempt(&state, &captured.username)?;
    password_update(state, Some(captured.clone()), move |data| {
        if data
            .authenticate(&captured.username, &input.current_password)
            .is_none()
        {
            return Err(error(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "invalid login name or password",
            ));
        }
        data.set_password(
            &captured.username,
            &input.new_password,
            aede_core::clock::now_seconds(),
        )
        .map_err(refused)?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

fn lock_accounts(state: &ApiState) -> Result<StoreLock, ApiError> {
    StoreLock::try_acquire(&state.data_dir).map_err(|failure| {
        if failure.kind() == std::io::ErrorKind::WouldBlock {
            error(
                StatusCode::CONFLICT,
                "store_busy",
                "Aède data is busy; retry after the current write",
            )
        } else {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "account data could not be locked",
            )
        }
    })
}

fn admin_capture(state: &ApiState, request: &Request) -> Result<Option<auth::Principal>, ApiError> {
    require_admin(state, request)?;
    personal::no_query(request)?;
    Ok(request.extensions().get::<auth::Principal>().cloned())
}

fn check_capture(state: &ApiState, captured: Option<&auth::Principal>) -> Result<(), ApiError> {
    if let Some(captured) = captured {
        auth::recheck(state, captured)?;
    }
    Ok(())
}

async fn password_update<T: Send + 'static>(
    state: ApiState,
    captured: Option<auth::Principal>,
    operation: impl FnOnce(&mut Accounts) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let permit = state
        .auth
        .password_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| auth::busy())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _guard = lock_accounts(&state)?;
        check_capture(&state, captured.as_ref())?;
        let mut data = enabled_accounts(&state)?;
        let result = operation(&mut data)?;
        accounts::save(&data, &accounts::accounts_path(&state.data_dir)).map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "account data could not be saved",
            )
        })?;
        Ok(result)
    })
    .await
    .map_err(|_| auth::busy())?
}

async fn read_accounts<T: Send + 'static>(
    state: ApiState,
    captured: Option<auth::Principal>,
    operation: impl FnOnce(&Accounts) -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let data = auth::current_accounts(&state).await?.ok_or_else(missing)?;
    if let Some(captured) = captured.as_ref() {
        auth::principal(
            &state,
            &data,
            auth::session_token(captured),
            false,
            Instant::now(),
        )?;
    }
    operation(&data)
}

async fn list_accounts(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<Vec<AccountView>>, ApiError> {
    let captured = admin_capture(&state, &request)?;
    personal::empty_body(&personal::body(request).await?)?;
    read_accounts(state, captured, |data| {
        Ok(Json(data.all().iter().map(account_view).collect()))
    })
    .await
}

fn path_name(input: Result<PathParameter<String>, PathRejection>) -> Result<String, ApiError> {
    let PathParameter(name) = input.map_err(|_| refused("invalid login-name path"))?;
    accounts::login_name(&name).map_err(refused)
}

async fn account_info(
    State(state): State<ApiState>,
    input: Result<PathParameter<String>, PathRejection>,
    request: Request,
) -> Result<Json<AccountView>, ApiError> {
    let captured = admin_capture(&state, &request)?;
    personal::empty_body(&personal::body(request).await?)?;
    let name = path_name(input)?;
    read_accounts(state, captured, move |data| {
        data.find(&name)
            .map(account_view)
            .map(Json)
            .ok_or_else(missing)
    })
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateAccount {
    username: String,
    password: String,
    role: String,
}

async fn create_account(
    State(state): State<ApiState>,
    request: Request,
) -> Result<(StatusCode, Json<AccountView>), ApiError> {
    let captured = admin_capture(&state, &request)?;
    let input: CreateAccount = personal::json_body(request).await?;
    let role =
        Role::parse(&input.role).ok_or_else(|| refused("role must be admin, user or auditor"))?;
    password_update(state, captured, move |data| {
        data.create(
            &input.username,
            &input.password,
            role,
            aede_core::clock::now_seconds(),
        )
        .map_err(refused)?;
        let account = data.find(&input.username).ok_or_else(missing)?;
        Ok((StatusCode::CREATED, Json(account_view(account))))
    })
    .await
}

fn non_null<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountPatch {
    #[serde(default, deserialize_with = "non_null")]
    username: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    role: Option<String>,
    #[serde(default, deserialize_with = "non_null")]
    enabled: Option<bool>,
    #[serde(default, deserialize_with = "non_null")]
    password: Option<String>,
}

async fn update_account(
    State(state): State<ApiState>,
    input: Result<PathParameter<String>, PathRejection>,
    request: Request,
) -> Result<Json<AccountView>, ApiError> {
    let captured = admin_capture(&state, &request)?;
    let name = path_name(input)?;
    let patch: AccountPatch = personal::json_body(request).await?;
    if patch.username.is_none()
        && patch.role.is_none()
        && patch.enabled.is_none()
        && patch.password.is_none()
    {
        return Err(refused("provide at least one account field"));
    }
    password_update(state, captured, move |data| {
        if data.find(&name).is_none() {
            return Err(missing());
        }
        let now = aede_core::clock::now_seconds();
        let mut current_name = name;
        if let Some(name) = patch.username {
            data.rename(&current_name, &name, now).map_err(refused)?;
            current_name = accounts::login_name(&name).map_err(refused)?;
        }
        if let Some(role) = patch.role {
            data.set_role(
                &current_name,
                Role::parse(&role).ok_or_else(|| refused("role must be admin, user or auditor"))?,
                now,
            )
            .map_err(refused)?;
        }
        if let Some(enabled) = patch.enabled {
            data.set_enabled(&current_name, enabled, now)
                .map_err(refused)?;
        }
        if let Some(password) = patch.password {
            data.set_password(&current_name, &password, now)
                .map_err(refused)?;
        }
        data.find(&current_name)
            .map(account_view)
            .map(Json)
            .ok_or_else(missing)
    })
    .await
}

async fn revoke_sessions(
    State(state): State<ApiState>,
    input: Result<PathParameter<String>, PathRejection>,
    request: Request,
) -> Result<StatusCode, ApiError> {
    let captured = admin_capture(&state, &request)?;
    let name = path_name(input)?;
    personal::empty_body(&personal::body(request).await?)?;
    password_update(state, captured, move |data| {
        if data.find(&name).is_none() {
            return Err(missing());
        }
        data.revoke(&name, aede_core::clock::now_seconds())
            .map_err(refused)?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

#[cfg(test)]
#[path = "accounts_api_tests.rs"]
mod tests;
