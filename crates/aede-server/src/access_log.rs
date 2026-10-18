//! Opt-in, bounded HTTP summaries. Never record URIs, queries, headers or bodies.
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use serde::Serialize;
use std::io::Write;
use std::{
    sync::mpsc::{self, SyncSender},
    time::Instant,
};

#[derive(Clone)]
pub(super) struct AccessLog(SyncSender<Record>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Record {
    method: &'static str,
    route: String,
    status: u16,
    elapsed_ms: u64,
}
#[derive(Clone)]
struct RouteLabel(String);
impl AccessLog {
    pub(super) fn from_environment() -> Option<Self> {
        if std::env::var_os("AEDE_REQUEST_LOG").as_deref() != Some(std::ffi::OsStr::new("1")) {
            return None;
        }
        let (sender, receiver) = mpsc::sync_channel::<Record>(128);
        // A slow terminal must not block the async HTTP workers or grow an unbounded queue.
        std::thread::spawn(move || {
            for record in receiver {
                if let Ok(line) = serde_json::to_string(&record)
                    && writeln!(std::io::stderr().lock(), "AEDE_HTTP {line}").is_err()
                {
                    break;
                }
            }
        });
        Some(Self(sender))
    }
}
pub(super) async fn label(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned());
    let mut response = next.run(request).await;
    if let Some(route) = route {
        response.extensions_mut().insert(RouteLabel(route));
    }
    response
}
pub(super) async fn record(State(log): State<AccessLog>, request: Request, next: Next) -> Response {
    let method = match request.method().as_str() {
        "GET" => "GET",
        "HEAD" => "HEAD",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "OPTIONS" => "OPTIONS",
        _ => "OTHER",
    };
    let start = Instant::now();
    let response = next.run(request).await;
    let record = Record {
        method,
        route: response
            .extensions()
            .get::<RouteLabel>()
            .map_or_else(|| "unmatched/rejected".into(), |label| label.0.clone()),
        status: response.status().as_u16(),
        elapsed_ms: start.elapsed().as_millis().min(u64::MAX as u128) as u64,
    };
    // Logs are diagnostic, not an audit ledger: saturated queues drop summaries.
    let _ = log.0.try_send(record);
    response
}
#[cfg(test)]
#[path = "access_log_tests.rs"]
mod tests;
