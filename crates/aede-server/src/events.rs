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
) -> Result<Response, ApiError> {
    upgrade_notifications(ws, state, false)
}

pub(super) async fn activity(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
) -> Result<Response, ApiError> {
    upgrade_notifications(ws, state, true)
}

pub(super) fn upgrade_notifications(
    ws: WebSocketUpgrade,
    state: ApiState,
    include_tasks: bool,
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
    Ok(ws
        .max_message_size(MAX_WEBSOCKET_MESSAGE)
        .max_frame_size(MAX_WEBSOCKET_MESSAGE)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            event_stream(socket, state, include_tasks).await;
        }))
}

pub(super) async fn event_stream(mut socket: WebSocket, state: ApiState, include_tasks: bool) {
    let mut receiver = state.events.subscribe();
    let mut shutdown = state.shutdown.subscribe();
    let scanned_at = state.catalog.read().await.as_ref().map(|c| c.scanned_at);
    let initial = CatalogEvent::Snapshot { scanned_at };
    if send_event(&mut socket, &initial).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            _ = shutdown.recv() => {
                let _ = send_message(&mut socket, Message::Close(None)).await;
                break;
            }
            event = receiver.recv() => match event {
                Ok(event) => {
                    if (include_tasks || matches!(event, CatalogEvent::CatalogChanged { .. }))
                        && send_event(&mut socket, &event).await.is_err()
                    {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(Message::Text(_) | Message::Binary(_))) => {
                    let _ = send_message(&mut socket, Message::Close(Some(CloseFrame {
                        code: close_code::POLICY,
                        reason: "notification sockets accept no application messages".into(),
                    }))).await;
                    break;
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {},
            },
        }
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
