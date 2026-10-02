# Follow changes with WebSocket

HTTP gives a result when requested. A WebSocket keeps a connection open so the server can notify a client of changes. These streams send notifications, not audio, catalog contents or commands. Their local Host/Origin boundary is the same as [HTTP](http.md).

## GET /api/v1/events — WebSocket upgrade

Connect a WebSocket client to `ws://127.0.0.1:8787/api/v1/events`. An ordinary browser address bar or plain curl GET does not establish the required upgrade. In a JavaScript console of a page served from the same local origin, a minimal client is:

```js
const events = new WebSocket('ws://127.0.0.1:8787/api/v1/events');
events.addEventListener('message', event => console.log(JSON.parse(event.data)));
```

A page on an unrelated website is refused; do not run this expecting the public Aède website to control a localhost server. A native WebSocket client can omit Origin.

The first message is `{"type":"snapshot","scanned_at":1234567890}`. Later successful catalog replacements or removal emit `{"type":"catalog_changed","scanned_at":1234567890}`. The example timestamp is illustrative; either message can have `scanned_at:null` when no catalog is loaded. After a change, re-read the relevant HTTP pages: the message is not a partial graph update. This original stream has **only** these catalog messages; it does not gain task types.

## GET /api/v1/activity — WebSocket upgrade

Connect to `ws://127.0.0.1:8787/api/v1/activity` when your client also needs background-task lifecycle. It starts with the same snapshot and includes catalog changes, plus:

| `type` | Fields besides type | Meaning |
| --- | --- | --- |
| `task_started` | `task_id,task_kind` | An accepted operation began; it may still wait for the writer lock. |
| `task_progress` | `task_id,task_kind,phase,done,total` | Progress or a lifecycle phase. |
| `task_completed` | `task_id,task_kind,scanned_at` | Work completed; catalog reload was attempted before notification. |
| `task_failed` | `task_id,task_kind,code,message` | Work failed or was cancelled; no completion follows. |
| `error` | `operation,code,message` | Background work without an ID failed, currently catalog reload. |

`task_kind` is `scan`, `identification` for fetch, or `command` for other delegated mutations. IDs are unique within this server process, not durable catalog IDs. Ignore unknown types, task kinds, phases and fields to remain compatible.

```json
{"type":"task_progress","task_id":1,"task_kind":"scan","phase":"running","done":0,"total":0}
```

For asynchronous HTTP and delegated CLI tasks, `running` and `refreshing` use **unknown totals** (`done:0,total:0`); do not display this as 0% or completed. `refreshing` means publishing the resulting catalog. The older synchronous bodyless HTTP scan instead emits `discovered` (both counts equal discovered files), then `reading` for files needing fresh tags. Reading updates are roughly four per second plus the final value. Detailed per-item fetch output remains in the CLI or authenticated [HTTP task result](jobs.md).

Failure codes include `scan_failed`, `store_error`, `catalog_unavailable`, `command_failed`, `task_cancelled` and `catalog_reload_failed`. Messages explain errors but are not stable decision keys. A rejected request never starts a task. `task_completed` can precede a later `catalog_changed` if another writer prevented immediate publication.

## Reconnect and limits

Notifications are best effort, have no replay and do not guarantee every intermediate change. When disconnected, reconnect with a delay that grows after repeated failures. A new connection gives a current catalog snapshot, not prior task history. Poll a known authenticated HTTP job to recover its state/result. Do not assume that losing a socket cancelled work.

Both streams share a maximum of 64 connections; excess handshakes return `503 connection_limit`. Incoming frames/messages are limited to 1 KiB. Application text/binary messages close the connection: neither stream accepts commands. Standard ping/pong/close remain supported. A send unable to finish within five seconds closes that client. Close unused connections. Upgrade failures are transport errors and may not use the JSON error envelope; ordinary HEAD is not a substitute for a WebSocket handshake.

Full protocol details: [API event contract](../api.md#activity-stream).
