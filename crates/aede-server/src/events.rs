//! Bounded WebSocket notification streams.

use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum CatalogEvent {
    Snapshot {
        scanned_at: Option<u64>,
    },
    CatalogChanged {
        scanned_at: Option<u64>,
    },
    TaskStarted {
        task_id: u64,
        task_kind: &'static str,
    },
    TaskProgress {
        task_id: u64,
        task_kind: &'static str,
        phase: &'static str,
        done: usize,
        total: usize,
    },
    TaskCompleted {
        task_id: u64,
        task_kind: &'static str,
        scanned_at: Option<u64>,
    },
    TaskFailed {
        task_id: u64,
        task_kind: &'static str,
        code: &'static str,
        message: String,
    },
    Error {
        operation: &'static str,
        code: &'static str,
        message: String,
    },
}

pub(super) async fn events(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    principal: Option<axum::Extension<auth::Principal>>,
    admin: Option<axum::Extension<auth::AdministrativeAccess>>,
) -> Result<Response, ApiError> {
    upgrade_notifications(
        ws,
        state,
        false,
        auth::SocketIdentity {
            session: principal.map(|p| p.0),
            administrative: admin.is_some(),
        },
    )
}

pub(super) async fn activity(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
    principal: Option<axum::Extension<auth::Principal>>,
    admin: Option<axum::Extension<auth::AdministrativeAccess>>,
) -> Result<Response, ApiError> {
    upgrade_notifications(
        ws,
        state,
        true,
        auth::SocketIdentity {
            session: principal.map(|p| p.0),
            administrative: admin.is_some(),
        },
    )
}

pub(super) fn upgrade_notifications(
    ws: WebSocketUpgrade,
    state: ApiState,
    include_tasks: bool,
    identity: auth::SocketIdentity,
) -> Result<Response, ApiError> {
    let permit = state
        .websocket_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "connection_limit",
                "too many notification connections",
            )
        })?;
    // An accepted upgrade must retain a shutdown sent before its callback runs.
    let shutdown = state.shutdown.subscribe();
    Ok(ws
        .max_message_size(MAX_WEBSOCKET_MESSAGE)
        .max_frame_size(MAX_WEBSOCKET_MESSAGE)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            event_stream(socket, state, include_tasks, identity, shutdown).await;
        }))
}

pub(super) async fn event_stream(
    mut socket: WebSocket,
    state: ApiState,
    include_tasks: bool,
    identity: auth::SocketIdentity,
    mut shutdown: crate::shutdown::Receiver,
) {
    let mut receiver = state.events.subscribe();
    let mut authorization = tokio::time::interval(Duration::from_secs(1));
    // The first snapshot can wait behind a catalog writer. Keep the same
    // revocation/shutdown boundary here as for every subsequent notification.
    let initial = async {
        if !auth::socket_live(&state, &identity).await {
            return None;
        }
        let catalog = state.catalog.read();
        tokio::pin!(catalog);
        let scanned_at = loop {
            tokio::select! {
                _ = authorization.tick() => {
                    if !auth::socket_live(&state, &identity).await { return None; }
                }
                catalog = &mut catalog => break catalog.as_ref().map(|c| c.scanned_at),
            }
        };
        if !auth::socket_live(&state, &identity).await {
            return None;
        }
        Some(CatalogEvent::Snapshot { scanned_at })
    };
    let Some(initial) = await_notification(&mut socket, &mut shutdown, initial).await else {
        return;
    };
    if send_event(&mut socket, &initial).await.is_err() {
        return;
    }
    loop {
        let next = async {
            loop {
                tokio::select! {
                    _ = authorization.tick() => {
                        if !auth::socket_live(&state, &identity).await {
                            return None;
                        }
                    }
                    event = receiver.recv() => match event {
                        Ok(event) => {
                            if !auth::socket_live(&state, &identity).await { return None; }
                            if include_tasks || matches!(event, CatalogEvent::CatalogChanged { .. }) {
                                return Some(event);
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    },
                }
            }
        };
        let Some(event) = await_notification(&mut socket, &mut shutdown, next).await else {
            return;
        };
        if send_event(&mut socket, &event).await.is_err() {
            return;
        }
    }
}

// Catalog and credential waits cannot prevent connection retirement. Detached
// blocking credential reads retain their own admission permits until they finish.
async fn await_notification(
    socket: &mut WebSocket,
    shutdown: &mut crate::shutdown::Receiver,
    prepared: impl Future<Output = Option<CatalogEvent>>,
) -> Option<CatalogEvent> {
    tokio::pin!(prepared);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.recv() => {
                let _ = send_message(socket, Message::Close(None)).await;
                return None;
            },
            event = &mut prepared => {
                if event.is_none() {
                    let _ = send_message(socket, Message::Close(Some(CloseFrame {
                        code: close_code::POLICY,
                        reason: "authentication expired or was revoked".into(),
                    }))).await;
                }
                return event;
            },
            incoming = socket.recv() => {
                if !notification_input(socket, incoming).await { return None; }
            },
        }
    }
}

// The initial catalog wait and the established stream share one input policy.
async fn notification_input(
    socket: &mut WebSocket,
    incoming: Option<Result<Message, axum::Error>>,
) -> bool {
    match incoming {
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => true,
        Some(Ok(Message::Text(_) | Message::Binary(_))) => {
            let _ = send_message(
                socket,
                Message::Close(Some(CloseFrame {
                    code: close_code::POLICY,
                    reason: "notification sockets accept no application messages".into(),
                })),
            )
            .await;
            false
        }
        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => false,
    }
}

pub(super) async fn send_event(socket: &mut WebSocket, event: &CatalogEvent) -> Result<(), ()> {
    let payload = serde_json::to_string(event).map_err(|_| ())?;
    send_message(socket, Message::Text(payload)).await
}

pub(super) async fn send_message(socket: &mut WebSocket, message: Message) -> Result<(), ()> {
    tokio::time::timeout(WEBSOCKET_SEND_TIMEOUT, socket.send(message))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
