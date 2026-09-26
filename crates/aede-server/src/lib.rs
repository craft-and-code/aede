//! The local HTTP and WebSocket API over the catalog.

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
    Requested,
    NotFound,
    NoServer,
    Unsupported,
}

#[cfg(not(unix))]
pub fn delegate_command(
    _data_dir: &Path,
    _args: Vec<String>,
    _command: &str,
) -> Result<Option<i32>, Box<dyn Error>> {
    Ok(None)
}

#[cfg(not(unix))]
pub fn cancel_task(_data_dir: &Path, _task_id: u64) -> Result<CancelOutcome, Box<dyn Error>> {
    Ok(CancelOutcome::Unsupported)
}

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 200;
const MAX_WEBSOCKETS: usize = 64;
const MAX_WEBSOCKET_MESSAGE: usize = 1024;
const WEBSOCKET_SEND_TIMEOUT: Duration = Duration::from_secs(5);

mod admin;
mod catalog;
mod catalog_commands;
mod errors;
mod events;
mod inspection;
mod jobs;
mod models;
mod personal;
mod query;
mod routing;
mod runtime;
mod security;
mod state;

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
pub use runtime::serve;
use runtime::{refresh_catalog, stamp};
use security::*;
use state::*;

#[cfg(test)]
mod test_support;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
