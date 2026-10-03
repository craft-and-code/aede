//! HTTP origin boundary and administrative credentials.

use super::*;

/// The authority which a TLS listener presents to remote clients.
///
/// It is kept as the exact configured spelling. HTTP host names are compared
/// case-insensitively, but alternate ports, URI forms and user information are
/// never accepted as aliases.
#[derive(Clone, Debug)]
pub(super) struct RemoteAuthority(String);

impl RemoteAuthority {
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        if value.is_empty()
            || !value.is_ascii()
            || value.contains(['/', '?', '#', '@', '\\'])
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err(
                "TLS authority must be a host and port, without a URL or user information".into(),
            );
        }
        let authority = value
            .parse::<axum::http::uri::Authority>()
            .map_err(|_| "TLS authority must be a valid host and port")?;
        let Some(port) = authority.port() else {
            return Err("TLS authority must include its explicit public port".into());
        };
        if port.as_u16() == 0 || port.as_str() != port.as_u16().to_string() {
            return Err("TLS authority must include a non-zero canonical public port".into());
        }
        if !valid_remote_host(authority.host()) {
            return Err("TLS authority must use a valid DNS host name or IP literal".into());
        }
        Ok(Self(authority.as_str().into()))
    }

    fn matches(&self, value: &str) -> bool {
        value
            .parse::<axum::http::uri::Authority>()
            .ok()
            .is_some_and(|authority| authority.as_str().eq_ignore_ascii_case(&self.0))
    }
}

fn valid_remote_host(host: &str) -> bool {
    if let Some(literal) = host
        .strip_prefix('[')
        .and_then(|literal| literal.strip_suffix(']'))
    {
        return literal.parse::<std::net::Ipv6Addr>().is_ok();
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
}

#[derive(Clone, Debug)]
pub(super) enum OriginPolicy {
    Local(SocketAddr),
    Remote(RemoteAuthority),
}

pub(super) fn require_admin<'a>(
    state: &'a ApiState,
    request: &Request,
) -> Result<&'a Admin, ApiError> {
    let admin = state
        .admin
        .as_ref()
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "not_found", "unknown API route"))?;
    if admin.token.is_empty() || !authorized(request.headers(), &admin.token) {
        if request
            .extensions()
            .get::<auth::Principal>()
            .is_some_and(|principal| principal.role == aede_core::accounts::Role::Administrator)
        {
            return Ok(admin);
        }
        if request.extensions().get::<auth::Principal>().is_some() {
            return Err(error(
                StatusCode::FORBIDDEN,
                "forbidden",
                "an administrator account is required",
            ));
        }
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "administrative token required",
        ));
    }
    Ok(admin)
}

pub(super) fn authorized(headers: &HeaderMap, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    // Browser-originated requests have no reason to use the administrative API.
    // Reject them even when a page somehow obtains the bearer token.
    if headers.contains_key(header::ORIGIN) {
        return false;
    }
    let mut credentials = headers.get_all(header::AUTHORIZATION).iter();
    let Some(provided) = credentials.next() else {
        return false;
    };
    if credentials.next().is_some() {
        return false;
    }
    let expected = format!("Bearer {token}");
    let actual = provided.as_bytes();
    if actual.len() != expected.len() {
        return false;
    }
    let differences = actual
        .iter()
        .zip(expected.as_bytes())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        });
    differences == 0
}

pub(super) fn local_authority(value: &str, address: SocketAddr) -> bool {
    let bare_address = match address.ip() {
        std::net::IpAddr::V4(ip) => ip.to_string(),
        std::net::IpAddr::V6(ip) => format!("[{ip}]"),
    };
    value.eq_ignore_ascii_case(&address.to_string())
        || value.eq_ignore_ascii_case(&format!("localhost:{}", address.port()))
        || (address.port() == 80
            && (value.eq_ignore_ascii_case(&bare_address)
                || value.eq_ignore_ascii_case("localhost")))
}

pub(super) fn same_authority(left: &str, right: &str, port: u16) -> bool {
    let left = if port == 80 {
        left.strip_suffix(":80").unwrap_or(left)
    } else {
        left
    };
    let right = if port == 80 {
        right.strip_suffix(":80").unwrap_or(right)
    } else {
        right
    };
    left.eq_ignore_ascii_case(right)
}

pub(super) async fn enforce_origin(
    State(policy): State<OriginPolicy>,
    request: Request,
    next: Next,
) -> Response {
    match policy {
        OriginPolicy::Local(address) => enforce_local_origin(address, request, next).await,
        OriginPolicy::Remote(authority) => enforce_remote_origin(authority, request, next).await,
    }
}

/// A remote request can drive synchronous catalog traversal. Limit it before
/// routing, then run the route on one of the bounded blocking workers so it
/// cannot occupy every Tokio worker or retain an unbounded async queue.
pub(super) async fn enforce_remote_request_budget(
    State(state): State<ApiState>,
    request: Request,
    next: Next,
) -> Response {
    let permit = match state.remote_request_slots.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "request_limit",
                "the remote request limit is reached; retry shortly",
            )
            .into_response();
        }
    };
    let runtime = tokio::runtime::Handle::current();
    let task = tokio::task::spawn_blocking(move || {
        // Keep the permit in this closure. If the caller reaches its response
        // deadline, this work may still be winding down, and admitting another
        // request early would defeat the concurrency bound.
        let _permit = permit;
        runtime.block_on(next.run(request))
    });
    match tokio::time::timeout(Duration::from_secs(15), task).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "request_failed",
            "the remote request could not be completed",
        )
        .into_response(),
        Err(_) => error(
            StatusCode::SERVICE_UNAVAILABLE,
            "request_timeout",
            "the remote request did not finish in time",
        )
        .into_response(),
    }
}

async fn enforce_local_origin(address: SocketAddr, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next().and_then(|value| value.to_str().ok());
    if hosts.next().is_some() || !host.is_some_and(|host| local_authority(host, address)) {
        return error(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "the Host must name this local listener",
        )
        .into_response();
    }
    // Absolute-form request targets must not carry a different authority either.
    if request.uri().authority().is_some_and(|authority| {
        !host.is_some_and(|host| same_authority(authority.as_str(), host, address.port()))
    }) || request
        .uri()
        .scheme_str()
        .is_some_and(|scheme| scheme != "http")
    {
        return error(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "the request target and Host must match",
        )
        .into_response();
    }
    let mut origins = headers.get_all(header::ORIGIN).iter();
    if let Some(origin) = origins.next() {
        let allowed = origin
            .to_str()
            .ok()
            .and_then(|origin| origin.strip_prefix("http://"))
            .is_some_and(|origin| {
                host.is_some_and(|host| same_authority(origin, host, address.port()))
            });
        if origins.next().is_some() || !allowed {
            return error(
                StatusCode::FORBIDDEN,
                "invalid_origin",
                "browser requests must come from this local origin",
            )
            .into_response();
        }
    }
    next.run(request).await
}

async fn enforce_remote_origin(
    authority: RemoteAuthority,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = hosts.next().and_then(|value| value.to_str().ok());
    if hosts.next().is_some() || !host.is_some_and(|host| authority.matches(host)) {
        return error(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "the Host must name the configured HTTPS listener",
        )
        .into_response();
    }
    // A TLS connection has an implicit HTTPS scheme. Absolute-form requests
    // must state that scheme and the same configured authority explicitly.
    if request
        .uri()
        .authority()
        .is_some_and(|request_authority| !authority.matches(request_authority.as_str()))
        || request
            .uri()
            .scheme_str()
            .is_some_and(|scheme| scheme != "https")
    {
        return error(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "the request target and Host must match the HTTPS listener",
        )
        .into_response();
    }
    let mut origins = headers.get_all(header::ORIGIN).iter();
    if let Some(origin) = origins.next() {
        let allowed = origin
            .to_str()
            .ok()
            .and_then(|origin| origin.strip_prefix("https://"))
            .is_some_and(|origin| authority.matches(origin));
        if origins.next().is_some() || !allowed {
            return error(
                StatusCode::FORBIDDEN,
                "invalid_origin",
                "browser requests must come from the configured HTTPS origin",
            )
            .into_response();
        }
    }
    next.run(request).await
}
