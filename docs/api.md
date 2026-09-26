# Aède HTTP API v1 (M2)

This is the frozen client contract for the current local, read-only API. A divergence between this document and the implementation is a bug. It is independent of the on-disk `format_version` values. The prefix `/api/v1` freezes the existing field names, types and meanings; compatible additions may add optional fields or new endpoints. Removing or reinterpreting a field, changing a required field's type, or changing query semantics requires a new prefix. Clients must ignore unknown response fields and use `error.code`, not `error.message`, for decisions.

Start it with `aede serve [--port N]` after `aede scan <folder>`. The default address is `127.0.0.1:8787`; `--port 0` asks the system for a free local port and prints it. The process reads the same JSON catalog as the CLI. Starting without a catalog fails. It never changes audio files or tags.

The [server README](../crates/aede-server/README.md) is the complete route and parameter reference, including the additive CLI-shaped navigation and administrative jobs described below. Its documented new response shapes are part of this v1 contract. Restart the server after upgrading its executable to use newly added routes.

## Access boundary

All `/api/v1` endpoints, including `/status` and the WebSocket, are read-only and require no credential. They can expose artist names, album titles, absolute file paths, stored tag comments and attributed artist origins. Any process or user account on the same computer can reach a loopback TCP listener; loopback is not per-user access control. Personal annotations and external source credentials are not exposed. The separate administrative routes described below are disabled unless a secret is configured.

The server binds only to IPv4 loopback. There is no public bind option, automatic port forwarding, TLS, CORS permission, or browser cross-origin access in v1. A request to expose the server on another interface must be an explicit future configuration change, with authentication, authorization and encrypted transport designed together. A reverse proxy or tunnel that publishes this API is outside this contract and must not be described as supported remote access. The catalog is shared; future private annotations will need owner-scoped authorization.

Every HTTP request and WebSocket handshake must have one `Host` naming `127.0.0.1` or `localhost` with the listener's actual port. Port 80 may be omitted when it is the actual port. Missing, duplicate, malformed or foreign authorities are rejected with `403 invalid_host` (or an HTTP-parser error before routing). An absolute-form request target must use HTTP and the same authority. Native clients can omit `Origin`; if present, exactly one HTTP origin matching that authority is required. Foreign, `null`, malformed and duplicate origins return `403 invalid_origin`. These checks prevent cross-origin WebSockets and requests using foreign DNS names; they do not authenticate local processes. The administrative route additionally refuses even a matching browser origin.

## Transport and representations

HTTP responses from successful API requests and application errors are UTF-8 JSON with `Content-Type: application/json`. `GET` is defined; ordinary JSON routes also answer `HEAD` with the same status and headers but no body. Unknown paths return JSON 404; other methods on known paths return JSON 405. The WebSocket handshake is the exception: HTTP upgrade failures are transport errors and do not promise the JSON error envelope. No write method is defined under `/api/v1`.

All field names use `snake_case`. Times named `scanned_at` are Unix seconds; durations are milliseconds; file sizes are bytes. Optional scalar values are `null`, not absent. Collections are arrays, including when empty. Names and titles preserve the spelling read from files. Relations between entities use stable `reference` tokens from `EntityRef`, never catalog vector indexes. A track reference is path-based and changes if the file moves. Clients must URL-encode a token when passing it as a query value.

| Request | Response |
| --- | --- |
| `GET /api/v1/status` | `{ "status": "ok", "api_version": 1, "catalog_loaded": bool }`, including while the catalog is absent after server startup. |
| `GET /api/v1/library` | `{ "scanned_at": u64, "files": usize, "artists": usize, "releases": usize, "recordings": usize, "tracks": usize }`. |
| `GET /api/v1/artists` | Page of artist summaries. |
| `GET /api/v1/releases` | Page of release summaries. |
| `GET /api/v1/tracks` | Page of track summaries. |
| `GET /api/v1/recordings` | Page of recording summaries. |
| `GET /api/v1/entities?ref=<token>` | One entity detail, selected by a stable token. |
| `GET /api/v1/events` | WebSocket catalog notifications, unchanged from the frozen v1 contract. |
| `GET /api/v1/activity` | WebSocket catalog and task activity notifications. |

Every list response has `{ "items": [...], "total": usize, "offset": usize, "limit": usize, "scanned_at": u64 }`. `total` counts rows **after** search and filters, before pagination. `items` is sliced after sorting. `offset` and `limit` echo the effective values. A page beyond the end has `items: []` and the filtered `total`. All fields in the following summary and detail tables are required in their respective shape, even when a value is `null` or an array is empty.

| Summary | Fields |
| --- | --- |
| Artist | `reference: string`, `name: string`, `sort_name: string`, `mbid: string|null`, `aliases: string[]`. |
| Release | `reference: string`, `title: string`, `year: u32|null`, `album_artist: reference|null`, `track_count: usize`, `cover_path: string|null`. |
| Track | `reference: string`, `title: string`, `release: reference|null`, `recording: reference|null`, `duration_ms: u64|null`. |
| Recording | `reference: string`, `title: string`, `mbid: string|null`, `track_count: usize`, `work_count: usize`. |

`/entities` returns one object with `kind` and `reference` plus these fields. The recognized kinds are `artist`, `release`, `track`, `recording`, `work`, `release_group`, `label`, and `genre`.

| Kind | Additional fields |
| --- | --- |
| `artist` | `name`, `sort_name`, `mbid`, `aliases`, `releases: reference[]`. |
| `release` | `title`, `year`, `album_artist: reference|null`, `tracks: reference[]`, `release_group: reference|null`, `labels: reference[]`, `cover_path: string|null`. |
| `track` | `title`, `release: reference|null`, `recording: reference|null`, `duration_ms: u64|null`, `path: string`, `size: u64`. |
| `recording` | `title`, `isrc: string|null`, `mbid: string|null`, `tracks: reference[]`, `works: reference[]`. |
| `work` | `title`, `mbid: string`, `recordings: reference[]`. |
| `release_group` | `title`, `mbid: string`, `releases: reference[]`. |
| `label` | `name`, `mbid: string|null`, `releases: reference[]`. |
| `genre` | `name`, `releases: reference[]`, `tracks: reference[]`. |

The reference arrays in an entity detail are complete, not paginated. Clients that only need a listing should use a paginated list endpoint and its filters. A future paginated relation subresource can be added without changing these v1 fields.

### CLI-shaped additions

`/albums` exposes albums with CLI-shaped artist/genre/label/name/year filters, while `/releases` keeps its original query meanings. Singular `/album`, `/artist`, `/track`, `/recording`, `/work`, `/release-group`, `/genre` and `/label` select exactly one entity by `ref` or `name`. Exact normalized matches take precedence over partial matches; ambiguity returns `409 ambiguous_entity` with up to 200 `{reference,name}` candidates in `error.candidates`, never an arbitrary first result. These details use the entity fields above; `/artist` adds an `origin` object. `/from` returns that artist's reference, name and origin alone, explicitly `known` or `unknown` with an explanatory message and source attribution. Only already-stored, trusted MusicBrainz facts establish an origin.

`/genres`, `/labels`, `/works`, `/release-groups`, `/countries`, `/years` and `/roles` expose further paginated navigation. `/doctor`, `/stats` and `/roots` provide structured diagnostics and inspection; `/search` ranks names and optionally stored comments; `/query` evaluates the public subset of the CLI expression grammar. Owner-dependent predicates/sorts and lyrics queries are refused. None of these reads fetches external information. See the [complete parameters and shapes](../crates/aede-server/README.md#read-routes).

New navigation/inspection requests share two bounded blocking-worker slots, returning `429 inspection_busy` at capacity. Search/query text is bounded to 2048 bytes and query parsing to 64 complexity units. These are local safeguards, not a general Internet-facing request budget. Doctor reads current catalog/conclusions and sources under the shared writer lock (`409 store_busy` when unavailable); other endpoints use the cached catalog and, when needed, the latest atomically saved source file. Corrupt sources return an error, not an unknown-origin answer. No multi-request or catalog/source snapshot is guaranteed for the latter reads.

## Search, filters, sorting and pagination

These parameters apply to the four list endpoints. Unknown parameters, duplicate parameters, an empty `q`, unsupported sort values and malformed references are errors. Search and filters combine with AND; each matching row appears once. Text comparison uses Aède's normalized matching (case and accent insensitive) and checks for a substring. Search only examines the fields below; it does not silently search unrelated tags or external source claims.

| List | `q` searches | Exact filters | Accepted `sort` |
| --- | --- | --- | --- |
| `/artists` | `name` and `aliases` | `mbid=<string>` | `catalog` (default), `name` (by `sort_name`). |
| `/releases` | `title` | `year=<u32>`, `artist=<artist reference>` | `catalog` (default), `title`, `year`. |
| `/tracks` | `title` | `release=<release reference>` | `catalog` (default), `title`. |
| `/recordings` | `title` | `work=<work reference>` | `catalog` (default), `title`. |

`order=asc|desc` defaults to `asc` and applies to any accepted sort. `catalog` means the deterministic order saved by the scan. Text sorts compare normalized text, then catalog order to break ties. `year` places releases without a year last in both directions; equal years are ordered by normalized title, then catalog order. Filters using references require the expected kind. A syntactically valid but absent reference returns `404 entity_not_found`; a malformed or wrong-kind reference returns `400 invalid_query`. An exact `mbid` filter is case-sensitive.

The `artist` filter on releases matches the album artist, not every credited artist. The `release` filter on tracks matches their containing release. The `work` filter on recordings matches an explicitly identified work. `year` is the release's stored year, and `mbid` is the artist's stored identifier. No filter consults fetched claims or private annotations.

`offset` defaults to 0. `limit` defaults to 50 and must be between 1 and 200. They are non-negative decimal integers with no sign or whitespace. Search and filters run first, then sorting, then the page slice. The catalog can change between requests: clients comparing pages should compare `scanned_at` and restart paging after a change. The API makes no cross-request snapshot guarantee.

## Errors and versioning

Application errors are `{ "error": { "code": string, "message": string } }`. `code` is stable within v1; `message` is for people and may change. The defined outcomes are:

| HTTP | `error.code` | Meaning |
| --- | --- | --- |
| 400 | `invalid_query` | Unknown, duplicate or invalid search/filter/sort/order parameter. |
| 400 | `invalid_pagination` | Bad `offset` or `limit`. |
| 400 | `invalid_reference` | Missing or malformed `/entities` reference. |
| 403 | `invalid_host` | The request authority is not this local listener or is ambiguous. |
| 403 | `invalid_origin` | A browser origin is foreign, malformed or ambiguous. |
| 404 | `entity_not_found` | Well-formed reference absent from the current catalog. |
| 404 | `not_found` | Unknown path. |
| 405 | `method_not_allowed` | A known path was called with an unsupported HTTP method. |
| 409 | `ambiguous_entity` | A singular name selects several entities; bounded candidates accompany the error. |
| 409 | `store_busy` | Doctor or the synchronous administrative scan cannot acquire the data lock. |
| 429 | `inspection_busy` | The new navigation/inspection worker budget is full. |
| 500 | `sources_unavailable` | A stored source file could not be read. |
| 500 | `catalog_read_failed` | Doctor could not read catalog/conclusions. |
| 500 | `inspection_failed` | A background inspection worker failed. |
| 503 | `catalog_unavailable` | The catalog was removed while the server was running. |
| 503 | `connection_limit` | The shared WebSocket connection limit has been reached. |

The server checks `catalog.json` approximately once per second. A successful replacement swaps the in-memory snapshot. An unreadable replacement leaves the previous snapshot in service and is logged to standard error; removing the file makes catalog endpoints return 503 until a new catalog is loaded. `/status` remains available. Unexpected transport/runtime failures are outside the application error envelope.

The `/api/v1/events` WebSocket sends `{ "type": "snapshot", "scanned_at": u64|null }` immediately after connection and `{ "type": "catalog_changed", "scanned_at": u64|null }` on successful replacement or removal. `null` means no catalog is loaded. This frozen stream does not gain task or error message types. Events carry no partial graph update: clients re-read HTTP pages. Notifications are best effort; a reconnect receives a fresh snapshot, and clients must not rely on receiving every intermediate change. The socket accepts no commands.

The two notification routes share a limit of 64 open WebSockets. Additional handshakes return `503 connection_limit` until a connection closes; ordinary HTTP reads remain available. Incoming frames and messages are limited to 1 KiB, and any text or binary application message closes the connection; standard ping, pong and close frames remain supported. A send that cannot finish within five seconds closes that connection. These are local resource safeguards, not a complete Internet-facing rate-limit policy: HTTP search and sort concurrency still need budgets before remote access is supported.

## Activity stream

`GET /api/v1/activity` is a separate WebSocket for clients that want task state as well as catalog changes. It starts with the same `snapshot` message and includes `catalog_changed` with the same meaning as `/api/v1/events`. It adds these JSON messages:

| `type` | Fields | Meaning |
| --- | --- | --- |
| `task_started` | `task_id: u64`, `task_kind: string` | An accepted task began; a delegated command may still be waiting for the data-folder lock. |
| `task_progress` | `task_id`, `task_kind`, `phase: string`, `done: usize`, `total: usize` | Progress for that task. |
| `task_completed` | `task_id`, `task_kind`, `scanned_at: u64|null` | The operation completed. The server attempts a catalog reload before this event; if another writer holds the lock, `catalog_changed` may follow later. |
| `task_failed` | `task_id`, `task_kind`, `code: string`, `message: string` | The operation failed; no completion message follows. |
| `error` | `operation: string`, `code: string`, `message: string` | A background operation without a task ID failed, currently a catalog reload. |

`task_id` is unique within one server process but is not durable or sequential across restarts; do not use it as a catalog identifier. Task kinds are `scan` for scans, `identification` for a delegated CLI or asynchronous HTTP `fetch`, and `command` for other delegated CLI mutations. A synchronous administrative scan emits `task_started`, `task_progress` (phase `discovered`, then `reading` when fresh files need reading), `catalog_changed`, then `task_completed`. In `discovered`, `done` and `total` both equal the number of audio files found. In `reading`, they count files that require fresh tag reads, not every discovered file. Reading progress is throttled to roughly four messages per second, with the final value sent. Delegated CLI commands and asynchronous HTTP jobs emit start and terminal events, but not detailed WebSocket progress yet. Detailed output goes to the connected CLI or the authenticated HTTP task result. A delegated task can be waiting for the shared data lock after `task_started`. A rejected request, including `401 unauthorized` or `409 store_busy`, never starts a task and emits no task messages.

Codes currently emitted are `scan_failed`, `store_error` or `catalog_unavailable` for a failed administrative scan, `command_failed` for a delegated CLI command or asynchronous HTTP job, `task_cancelled` for a delegated or HTTP scan/fetch stopped by its user, and `catalog_reload_failed` for a background reload. `message` is for display, not programmatic decisions. Clients must ignore unknown activity message types, task kinds, phases and fields so future operations remain additive. This stream is best effort and has no replay: on reconnect, clients receive a fresh catalog snapshot, not the prior task history or guaranteed current task status. Neither WebSocket accepts commands. Messages never contain the full catalog; read the HTTP endpoints after `catalog_changed`.

The `/api/v1` prefix and `api_version: 1` are independent of the program version and JSON store versions. Additive endpoints and optional response fields may appear in v1. Existing required fields, error codes and query meanings remain stable. A breaking change receives `/api/v2`; old clients can continue to use v1 while it is served.

## Administrative work (separate, opt-in API)

`POST /api/admin/v1/scan` with **no body** preserves the original synchronous behavior: rescan existing watched roots, including saved artist merges and independent conclusions, and return `{ "status": "completed", "scanned_at": u64, "files": usize }` once saved and published. Its writer lock remains held through snapshot publication. A busy writer returns `409 store_busy`; scan/storage failures return `500 scan_failed` or `500 store_error`.

An object body (even `{}`) selects the new asynchronous scan mode. `POST /api/admin/v1/fetch` also takes an object and starts a typed asynchronous job. Fields correspond to CLI options, including scan folders/full/thread settings and explicit fetch passes/targets. Accepted submissions return `202 {task_id,status:"queued",status_url}`. `GET /api/admin/v1/tasks/{id}` returns current status and bounded output; `POST /api/admin/v1/tasks/{id}/cancel` with no body requests cancellation. These jobs wait for the existing writer lock, survive HTTP client disconnection and are awaited at shutdown. At most four can be active; 64 records are retained in memory, with terminal records evicted first. No job history survives restart. Neither polling nor HTTP cancellation addresses delegated CLI jobs or synchronous scans. See the [administrative route reference](../crates/aede-server/README.md#administrative-routes) for all input fields, states, outputs and limits.

Task administration accepts no query parameters (`400 invalid_query`). Object bodies must fit within 16 KiB and finish in one second; malformed/unknown fields return `400 invalid_body`, semantic option errors `400 invalid_parameters`, and body timeouts `408 request_timeout`. Capacity exhaustion returns `503 task_limit`, absent/evicted tasks `404 task_not_found`, invalid IDs `400 invalid_task_id`, and cancellation of terminal work `409 task_not_cancellable`. Registry failure returns `500 task_error`. A cancellation request is not a rollback of earlier saved work. No arbitrary executable, argument list, data-directory override or credential override is accepted.

The same opt-in administrative boundary now exposes personal data for the single established `local` owner: annotation read/write by stable `ref`, listening-history read/write, and saved smart-collection read/write/delete. These routes are not public `/api/v1` routes and require the same bearer token without `Origin`. Their query parameters and JSON shapes are defined in the [server route reference](../crates/aede-server/README.md#personal-data-annotations-plays-and-smart-collections). They do not accept an owner identifier: this is a deliberate temporary binding until accounts authenticate an owner. Annotations may set favourites, 1–5 ratings, notes and tags, but never modify audio tags. Personal reads and writes load the current catalog and user data under the shared store lock; writes retain that lock through the atomic update of `user.json`. Lock contention is `409 store_busy`, unreadable user data is `500 user_unavailable`. Listening history is ordered and retained by event time, including delayed submissions; all-time counts include accepted events that are too old for the bounded log. Static playlists, relation annotations and account-scoped ownership remain future work.

None of these routes exist by default. Enable them by setting `AEDE_ADMIN_TOKEN` to a private ASCII secret of at least 32 characters **before** starting `aede serve`. Send one `Authorization: Bearer <token>` header on every administrative request, including personal-data reads. Never put the secret in a URL or share it with a browser page. An `Origin` header is refused: foreign origins fail the shared boundary with 403; even a matching origin receives `401 unauthorized`. Missing, duplicate or incorrect credentials return JSON `401 unauthorized`. Removing the token and restarting removes all administrative routes. The token authorizes the server account's scan/fetch and local-personal-data capabilities; it is not a restricted per-user account.

On Unix, CLI commands that may write Aède's stores automatically delegate to the running server for the same data folder. A private Unix socket handles this local command channel; it is not an HTTP endpoint or a remote-access mechanism. The server runs the command independently, streams its normal output back to the CLI, and lets it finish even if that CLI disconnects. This matters for long `fetch` runs, which already save after each answer. A delegated `scan` or `fetch` prints its server task ID; `aede cancel <task-id>` asks that same local server to stop it. Cancellation returns immediately and the original command exits with code 130 once stopped. A task that has already finished, an administrative HTTP scan, and other CLI commands are not cancellable through this command. The task ID is valid only for the current server process. Closing the CLI, including with Ctrl-C, does not itself cancel the task. Previously saved JSON data remain intact; an interrupted download may leave a hidden temporary sidecar, never a truncated final image or lyric. The socket lives in a private, mode-0700 directory under `/tmp` so long data-folder paths work; the data folder must not be group- or world-writable. No administrative token is needed for this same-user local channel. Without a server, CLI commands continue locally and `aede cancel` refuses. On Windows, local delegation and cancellation remain unavailable. Catalog path handling has regression coverage, but native Windows validation is still required before release (see [Paths](design/paths.md)).

The exclusive `.aede.lock` file in the data folder remains the final protection against lost updates: delegated subprocesses and CLI commands without a server hold it for the entire read/modify/write operation; `aede backup` holds it for a coherent multi-file snapshot. A synchronous administrative scan returns 409 instead of waiting if the lock is held; asynchronous HTTP jobs wait for it. Keep this file in place even when the server is stopped: removing it while another process holds it can defeat the lock. Atomic JSON replacement still protects readers from partial files. This is cooperative coordination between current Aède processes; hand-editing JSON or using an older Aède executable concurrently remains unsafe. The HTTP server reloads external CLI catalog changes approximately once per second.

The server handles Ctrl-C and SIGTERM by stopping new connections and closing active WebSockets. Accepted scans and fetches are allowed to finish before the process exits; cancel an unwanted asynchronous job before shutting down. The API is local-only even with the administrative token; the token is not a substitute for TLS and remote-access design.
