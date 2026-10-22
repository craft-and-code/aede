//! Compatibility endpoint for a synchronous watched-root rescan.

use super::*;

pub(super) async fn admin_scan(
    State(state): State<ApiState>,
    request: Request,
) -> Result<Json<ScanResult>, ApiError> {
    let admin = require_admin(&state, &request)?;
    if request.uri().query().is_some_and(|query| !query.is_empty()) {
        return Err(invalid_query(
            "administrative scans accept no query parameters",
        ));
    }
    tokio::time::timeout(Duration::from_secs(1), to_bytes(request.into_body(), 0))
        .await
        .map_err(|_| {
            error(
                StatusCode::REQUEST_TIMEOUT,
                "request_timeout",
                "request body did not finish",
            )
        })?
        .map_err(|_| {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_body",
                "administrative scans accept no request body",
            )
        })?;
    let admin = admin.clone();
    let reservation = state.jobs.reserve_synchronous_scan()?;
    let task_id = state.next_task_id.fetch_add(1, Ordering::Relaxed);
    let events = state.events.clone();
    let scan_activity = state.scan_activity.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        // The blocking worker outlives a cancelled HTTP waiter. Keep admission
        // with it, then return it alongside the writer lock through publication.
        let admission = reservation;
        let _guard = StoreLock::try_acquire(&admin.data_dir).map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock {
                error_message(StatusCode::CONFLICT, "store_busy", "Aède data is busy")
            } else {
                error_message(StatusCode::INTERNAL_SERVER_ERROR, "store_error", error)
            }
        })?;
        let scan_guard = scan_activity.start();
        let _ = events.send(CatalogEvent::TaskStarted {
            task_id,
            task_kind: "scan",
        });
        let mut last_read_event: Option<Instant> = None;
        let mut progress = |progress| {
            let (phase, done, total) = match progress {
                Progress::Discovered(count) => ("discovered", count, count),
                Progress::Read { done, total } => {
                    let now = Instant::now();
                    if done != total
                        && last_read_event.is_some_and(|last| {
                            now.duration_since(last) < Duration::from_millis(250)
                        })
                    {
                        return;
                    }
                    last_read_event = Some(now);
                    ("reading", done, total)
                }
            };
            let _ = events.send(CatalogEvent::TaskProgress {
                task_id,
                task_kind: "scan",
                phase,
                done,
                total,
            });
        };
        let outcome: Result<_, ApiError> = (|| {
            (admin.scan)(&mut progress).map_err(|message| {
                error_message(StatusCode::INTERNAL_SERVER_ERROR, "scan_failed", message)
            })?;
            let path = store::catalog_path(&admin.data_dir);
            let catalog = store::load(&path)
                .map_err(|error| {
                    error_message(StatusCode::INTERNAL_SERVER_ERROR, "store_error", error)
                })?
                .ok_or_else(unavailable)?;
            Ok((catalog, storage_stamp(&path)))
        })();
        match outcome {
            Ok((catalog, saved_stamp)) => {
                Ok(((_guard, scan_guard, admission), catalog, saved_stamp))
            }
            Err(error) => {
                let _ = events.send(CatalogEvent::TaskFailed {
                    task_id,
                    task_kind: "scan",
                    code: error.code,
                    message: error.message.clone(),
                });
                Err(error)
            }
        }
    })
    .await
    .map_err(|join_error| {
        let _ = state.events.send(CatalogEvent::TaskFailed {
            task_id,
            task_kind: "scan",
            code: "scan_failed",
            message: join_error.to_string(),
        });
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "scan_failed",
            join_error.to_string(),
        )
    })?;
    let (_work_guards, catalog, saved_stamp) = outcome?;
    // Keep other writers out until the just-saved snapshot is published. A
    // second writer must not turn a successful scan into a spurious reload error.
    let _reload = state.reload_gate.lock().await;
    let scanned_at = catalog.scanned_at;
    let files = catalog.files.len();
    {
        let mut published = state.catalog.write().await;
        *published = Some(catalog);
        state.parental.invalidate();
    }
    *state.loaded_stamp.write().await = saved_stamp;
    let _ = state.events.send(CatalogEvent::CatalogChanged {
        scanned_at: Some(scanned_at),
    });
    let _ = state.events.send(CatalogEvent::TaskCompleted {
        task_id,
        task_kind: "scan",
        scanned_at: Some(scanned_at),
    });
    Ok(Json(ScanResult {
        status: "completed",
        scanned_at,
        files,
    }))
}

#[cfg(test)]
#[path = "admin_tests.rs"]
mod tests;
