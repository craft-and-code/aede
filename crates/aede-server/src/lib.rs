//! Catalog HTTP/WebSocket API with explicit authenticated HTTPS and PCM playback.

use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use aede_core::model::{Catalog, EntityKind, Id};
use aede_core::scan::Progress;
use aede_core::store;
use aede_core::store_lock::StoreLock;
use aede_core::text;
use aede_core::user::EntityRef;
use axum::body::to_bytes;
use axum::extract::rejection::QueryRejection;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::http::{HeaderMap, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, Semaphore, broadcast};

#[cfg(unix)]
mod delegation;

#[cfg(unix)]
pub use delegation::{cancel_task, delegate_command};

/// Result of asking the local server to stop one cancellable task.
pub enum CancelOutcome {
    /// An active cancellable task received the stop request.
    Requested,
    /// No active cancellable task has this process-local identifier.
    NotFound,
    /// No server is listening on this data directory's local command channel.
    NoServer,
    /// The current platform has no local command channel implementation.
    Unsupported,
}

#[cfg(not(unix))]
/// Local command delegation is unavailable on this platform; callers execute
/// their command directly when this returns `Ok(None)`.
pub fn delegate_command(
    _data_dir: &Path,
    _args: Vec<String>,
    _command: &str,
) -> Result<Option<i32>, Box<dyn Error>> {
    Ok(None)
}

#[cfg(not(unix))]
/// Returns [`CancelOutcome::Unsupported`] where the Unix command channel is
/// unavailable. This does not cancel HTTP jobs or native PCM streams.
pub fn cancel_task(_data_dir: &Path, _task_id: u64) -> Result<CancelOutcome, Box<dyn Error>> {
    Ok(CancelOutcome::Unsupported)
}

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const MAX_CONNECTIONS: usize = 64;
// Catalog filtering and sorting can traverse the entire library. Keeping this
// small protects modest always-on hosts while work runs outside Tokio workers.
const MAX_REMOTE_REQUESTS: usize = 2;
const MAX_WEBSOCKETS: usize = 64;
pub(crate) const MAX_PLAYBACKS: usize = 4;
const MAX_WEBSOCKET_MESSAGE: usize = 1024;
const WEBSOCKET_SEND_TIMEOUT: Duration = Duration::from_secs(5);

mod accounts_api;
mod admin;
mod auth;
mod catalog;
mod catalog_commands;
/// Network-device controllers; prefer the standalone `aede-devices` crate.
/// This reexport preserves the original Rust API without duplicating it.
pub use aede_devices as devices;
mod errors;
mod events;
mod inspection;
mod jobs;
mod lyrics;
mod models;
mod personal;
mod playback_api;
mod query;
mod routing;
mod runtime;
mod security;
mod state;
mod subsonic;
mod tls;

pub use jobs::{FetchRequest, JobOutput, JobRequest, ScanRequest};

use admin::*;
use catalog::*;
use errors::*;
use events::*;
use models::*;
use query::*;
use routing::*;
#[cfg(test)]
use runtime::run_http;
use runtime::{refresh_catalog, stamp};
pub use runtime::{serve, serve_with_options};
use security::*;
use state::*;
use tls::*;
pub use tls::{ServerOptions, TlsOptions};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod accounts_test_support;

#[cfg(test)]
mod playback_test_support;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tls_tests;
