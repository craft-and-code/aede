//! Bounded, process-local sessions and the authenticated-owner boundary.

use std::collections::BTreeMap;
use std::sync::Mutex as BlockingMutex;
use std::sync::atomic::AtomicBool;

use aede_core::accounts::{self, Accounts, Role};

use super::*;

const ABSOLUTE_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);
const IDLE_LIFETIME: Duration = Duration::from_secs(30 * 60);
const RATE_WINDOW: Duration = Duration::from_secs(60);
const MAX_SESSIONS: usize = 256;
const MAX_ACCOUNT_SESSIONS: usize = 8;

pub(super) struct AuthState {
    configured: AtomicBool,
    sessions: BlockingMutex<Vec<Session>>,
    attempts: BlockingMutex<Attempts>,
    pub(super) password_slots: Arc<Semaphore>,
    read_slots: Arc<Semaphore>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self {
            configured: AtomicBool::new(false),
            sessions: BlockingMutex::new(Vec::new()),
            attempts: BlockingMutex::new(Attempts::default()),
            password_slots: Arc::new(Semaphore::new(2)),
            read_slots: Arc::new(Semaphore::new(8)),
        }
    }
}

#[derive(Clone)]
pub(super) struct Principal {
    pub(super) owner: String,
    pub(super) username: String,
    pub(super) role: Role,
    epoch: String,
    revision: u64,
    token: String,
}

#[derive(Clone, Default)]
pub(super) struct SocketIdentity {
    pub(super) session: Option<Principal>,
    pub(super) administrative: bool,
}

#[derive(Clone)]
pub(super) struct AdministrativeAccess;

struct Session {
    principal: Principal,
    created: Instant,
    last_seen: Instant,
}

impl Session {
    fn live(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.created) < ABSOLUTE_LIFETIME
            && now.saturating_duration_since(self.last_seen) < IDLE_LIFETIME
    }
}

#[derive(Default)]
struct Attempts {
    global: Option<(Instant, usize)>,
    names: BTreeMap<String, (Instant, usize)>,
}

impl Attempts {
    fn take(&mut self, name: &str, now: Instant) -> Result<(), ApiError> {
        self.names
            .retain(|_, (started, _)| now.saturating_duration_since(*started) < RATE_WINDOW);
        let global = self.global.get_or_insert((now, 0));
        if now.saturating_duration_since(global.0) >= RATE_WINDOW {
            *global = (now, 0);
        }
        if global.1 >= 100 || self.names.get(name).is_some_and(|(_, count)| *count >= 5) {
            return Err(error(
                StatusCode::TOO_MANY_REQUESTS,
                "login_limited",
                "too many login attempts; retry after one minute",
            ));
        }
        global.1 += 1;
        self.names.entry(name.into()).or_insert((now, 0)).1 += 1;
        Ok(())
    }
}

fn unavailable_accounts() -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "accounts_unavailable",
        "account credentials could not be read; ask the local operator",
    )
}

pub(super) fn unauthorized() -> ApiError {
    error(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "a valid account session is required",
    )
}

pub(super) fn busy() -> ApiError {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "authentication_busy",
        "authentication is busy; retry shortly",
    )
}

/// Missing credentials can never reopen a server that has seen account mode.
pub(super) fn load_accounts(state: &ApiState) -> Result<Option<Accounts>, ApiError> {
    match accounts::load(&accounts::accounts_path(&state.data_dir)) {
        Ok(Some(accounts)) => {
            state.auth.configured.store(true, Ordering::Release);
            Ok(Some(accounts))
        }
        Ok(None) if !state.auth.configured.load(Ordering::Acquire) => Ok(None),
        Ok(None) | Err(_) => {
            // A malformed first store also activates the fail-closed boundary.
            state.auth.configured.store(true, Ordering::Release);
            Err(unavailable_accounts())
        }
    }
}

pub(super) async fn current_accounts(state: &ApiState) -> Result<Option<Accounts>, ApiError> {
    let permit = state
        .auth
        .read_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| busy())?;
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        load_accounts(&state)
    })
    .await
    .map_err(|_| unavailable_accounts())?
}

pub(super) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let first = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    first
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty())
}

fn token_matches(left: &str, right: &str) -> bool {
    if left.len() != 64 || right.len() != 64 {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |different, (a, b)| different | (a ^ b))
        == 0
}

pub(super) fn principal(
    state: &ApiState,
    accounts: &Accounts,
    token: &str,
    touch: bool,
    now: Instant,
) -> Result<Principal, ApiError> {
    let mut sessions = state
        .auth
        .sessions
        .lock()
        .map_err(|_| unavailable_accounts())?;
    sessions.retain(|session| {
        session.live(now)
            && accounts
                .session_account(
                    &session.principal.owner,
                    &session.principal.epoch,
                    session.principal.revision,
                )
                .is_some()
    });
    let index = sessions
        .iter()
        .position(|session| token_matches(&session.principal.token, token))
        .ok_or_else(unauthorized)?;
    let session = &mut sessions[index];
    let Some(account) = accounts.session_account(
        &session.principal.owner,
        &session.principal.epoch,
        session.principal.revision,
    ) else {
        sessions.remove(index);
        return Err(unauthorized());
    };
    if touch {
        session.last_seen = now;
    }
    let mut principal = session.principal.clone();
    principal.username = account.username.clone();
    principal.role = account.role;
    Ok(principal)
}

/// Recheck an authenticated owner under the same writer lock as personal data.
pub(super) fn recheck(state: &ApiState, captured: &Principal) -> Result<(), ApiError> {
    let accounts = load_accounts(state)?.ok_or_else(unauthorized)?;
    principal(state, &accounts, &captured.token, false, Instant::now()).map(|_| ())
}

pub(super) fn request_principal(request: &Request) -> Result<Principal, ApiError> {
    request
        .extensions()
        .get::<Principal>()
        .cloned()
        .ok_or_else(unauthorized)
}

/// Refuse a state-changing operation for the explicit read-only account role.
pub(super) fn require_mutation(principal: &Principal) -> Result<(), ApiError> {
    if principal.role == Role::Auditor {
        return Err(error(
            StatusCode::FORBIDDEN,
            "forbidden",
            "an auditor account has read-only access",
        ));
    }
    Ok(())
}

pub(super) fn start_session(
    state: &ApiState,
    accounts: &Accounts,
    account: &accounts::Account,
) -> Result<(Principal, u64), ApiError> {
    let token = accounts::random_token().map_err(|_| unavailable_accounts())?;
    let now = Instant::now();
    let mut sessions = state
        .auth
        .sessions
        .lock()
        .map_err(|_| unavailable_accounts())?;
    sessions.retain(|session| {
        session.live(now)
            && accounts
                .session_account(
                    &session.principal.owner,
                    &session.principal.epoch,
                    session.principal.revision,
                )
                .is_some()
    });
    if sessions
        .iter()
        .filter(|session| session.principal.owner == account.id)
        .count()
        >= MAX_ACCOUNT_SESSIONS
        && let Some(index) = sessions
            .iter()
            .position(|session| session.principal.owner == account.id)
    {
        sessions.remove(index);
    }
    if sessions.len() >= MAX_SESSIONS {
        return Err(error(
            StatusCode::TOO_MANY_REQUESTS,
            "session_limit",
            "the server session limit is reached; log out an unused session",
        ));
    }
    let principal = Principal {
        owner: account.id.clone(),
        username: account.username.clone(),
        role: account.role,
        epoch: accounts.epoch().into(),
        revision: account.revision(),
        token,
    };
    sessions.push(Session {
        principal: principal.clone(),
        created: now,
        last_seen: now,
    });
    Ok((
        principal,
        aede_core::clock::now_seconds().saturating_add(ABSOLUTE_LIFETIME.as_secs()),
    ))
}

pub(super) fn end_session(state: &ApiState, captured: &Principal) -> Result<(), ApiError> {
    state
        .auth
        .sessions
        .lock()
        .map_err(|_| unavailable_accounts())?
        .retain(|session| !token_matches(&session.principal.token, &captured.token));
    Ok(())
}

pub(super) fn take_attempt(state: &ApiState, name: &str) -> Result<(), ApiError> {
    state
        .auth
        .attempts
        .lock()
        .map_err(|_| unavailable_accounts())?
        .take(name, Instant::now())
}

pub(super) fn session_token(principal: &Principal) -> &str {
    &principal.token
}

async fn check_request(state: &ApiState, request: &mut Request) -> Result<(), ApiError> {
    let accounts = current_accounts(state).await?;
    let path = request.uri().path().to_owned();
    let admin_token = state.admin.as_ref().is_some_and(|admin| {
        !admin.token.is_empty() && authorized(request.headers(), &admin.token)
    });
    if admin_token {
        request.extensions_mut().insert(AdministrativeAccess);
    }
    if path.starts_with("/api/admin/v1/")
        && accounts.is_none()
        && state
            .admin
            .as_ref()
            .is_none_or(|admin| admin.token.is_empty())
    {
        return Err(error(
            StatusCode::NOT_FOUND,
            "not_found",
            "unknown API route",
        ));
    }
    let Some(accounts) = accounts else {
        return Ok(());
    };
    if path == "/api/auth/v1/session" && request.method() == axum::http::Method::POST {
        return Ok(());
    }
    let personal = path.starts_with("/api/me/v1/") || path.starts_with("/api/auth/v1/");
    if !personal && admin_token {
        return Ok(());
    }
    if path.starts_with("/api/") {
        let token = bearer(request.headers()).ok_or_else(unauthorized)?;
        let captured = principal(state, &accounts, token, true, Instant::now())?;
        request.extensions_mut().insert(captured);
    }
    Ok(())
}

pub(super) async fn enforce_accounts(
    State(state): State<ApiState>,
    mut request: Request,
    next: Next,
) -> Response {
    let mut response = match check_request(&state, &mut request).await {
        Ok(()) => next.run(request).await,
        Err(error) => error.into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

/// Revalidate a WebSocket without treating server notifications as user activity.
pub(super) async fn socket_live(state: &ApiState, identity: &SocketIdentity) -> bool {
    // These waits are bounded by the 64 already-admitted sockets. Let ordinary
    // notification bursts queue instead of disconnecting healthy sessions.
    let permit = match tokio::time::timeout(
        Duration::from_secs(2),
        state.auth.read_slots.clone().acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        _ => return false,
    };
    let to_read = state.clone();
    let accounts = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        load_accounts(&to_read)
    })
    .await;
    match accounts {
        Ok(Ok(None)) => identity.session.is_none(),
        Ok(Ok(Some(accounts))) => {
            identity.administrative
                || identity.session.as_ref().is_some_and(|captured| {
                    principal(state, &accounts, &captured.token, false, Instant::now()).is_ok()
                })
        }
        Err(_) => false,
        Ok(Err(_)) => false,
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
