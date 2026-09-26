//! Local-origin boundary and administrative credentials.

use super::*;

pub(super) fn authorized(headers: &HeaderMap, token: &str) -> bool {
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
    value.eq_ignore_ascii_case(&address.to_string())
        || value.eq_ignore_ascii_case(&format!("localhost:{}", address.port()))
        || (address.port() == 80
            && (value == "127.0.0.1" || value.eq_ignore_ascii_case("localhost")))
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

pub(super) async fn enforce_local_origin(
    State(address): State<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
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
