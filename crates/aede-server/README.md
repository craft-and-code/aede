# Aède server — routes and examples

This crate implements the HTTP/JSON/WebSocket interface and optional direct HTTPS. The CLI supplies the scan/fetch execution callbacks; catalog and PCM/DSP logic stay in the shared core. See the [versioned API contract](../../docs/api.md) for compatibility, error envelopes, field definitions and the security boundary, and the [operating guide](../../docs/operating.md) for deployment.

The [Subsonic/OpenSubsonic adapter](../../docs/server/subsonic.md) adds GET/form-POST `/rest/{method}` and `/rest/{method}.view`, using persistent revocable API keys, XML/JSON envelopes, ID3 browsing/search, original audio with byte ranges, verified sidecar artwork and private favourites/ratings/static playlists. Client-declared scrobbles increment private counts without inventing played duration/completion; now-playing reports are temporary and owner-scoped. Native PCM and the graph keep their own contracts. A key may use `apiKey` or the legacy username/password fields, with the complete client key in the password field. The guide gives a macOS Supersonic setup using **Use legacy authentication**, original audio and **Title (A-Z)**, plus supported routes and honest remaining limits. Local Submariner 3.4 authentication, artist/album browsing, FLAC playback, ratings and artist/album favourites are confirmed on macOS. Supersonic's request shape has been checked in its source; broader client interoperability remains to be validated.

## Start and read albums

```sh
aede serve --port 3412
curl 'http://127.0.0.1:3412/api/v1/albums'
curl --get 'http://127.0.0.1:3412/api/v1/album' --data-urlencode 'name=Back in Black'
curl --get 'http://127.0.0.1:3412/api/v1/search' --data-urlencode 'q=AC/DC'
```

Use the same `--data <folder>` or `AEDE_HOME` as the CLI that scanned your music. The default port is 8787; 3412 above is an explicit choice. After rebuilding/upgrading Aède, restart the running server: replacing its executable does not replace the process already serving requests.

All routes below use English names matching the CLI vocabulary: an **album** is a graph **release** and a **track** is a local placement of a **recording**. `/releases` remains available to existing clients; `/albums` offers the CLI-shaped filters. No route translates or executes an arbitrary command string.

The default HTTP listener is **loopback-only**. Without accounts, local catalog reads are anonymous and reveal metadata/paths. Optional [accounts and sessions](../../docs/server/accounts.md) protect catalog reads and isolate personal data at `/api/me/v1`. An `admin` manages local accounts and installation work, a `user` may change only their own personal data, and an `auditor` may read only the catalog and their own personal views. Initialize the first administrator with `aede accounts init <name>` and its masked password confirmation; existing `local` data stays with that account. Once the running HTTP process has observed accounts, missing credentials fail closed until restart; unreadable credentials always fail closed. Restarting local HTTP without the store selects the legacy anonymous mode again. Explicit [HTTPS](../../docs/server/remote.md) requires accounts and disables every `/api/admin` route and legacy token, including with a loopback TLS listener. The [audio contract](../../docs/server/playback.md) uses shared PCM/DSP processing and acknowledged client frames for private listening history. Catalog reads never fetch external data or alter audio.

Account routes are additive and use the same error envelope as the catalog. The [complete account reference](../../docs/server/accounts.md) defines bodies, results, limits and expiry:

| Route | Methods and scope |
| --- | --- |
| `/api/auth/v1/session` | POST login; GET own metadata; DELETE logout |
| `/api/auth/v1/password` | PUT own password with current-password verification |
| `/api/admin/v1/accounts` | GET account metadata; POST create; administrator required |
| `/api/admin/v1/accounts/{username}` | GET metadata; PATCH name/role/status/password; administrator required |
| `/api/admin/v1/accounts/{username}/sessions` | DELETE all sessions; administrator required |
| `/api/me/v1/annotation` | GET/PUT own annotations by `ref` |
| `/api/me/v1/history` | GET/POST own history and counts |
| `/api/me/v1/collection` | GET/PUT/DELETE own collection by `name` |
| `/api/me/v1/collections` | GET own paginated collections |
| `/api/me/v1/playback` | GET WebSocket; user/admin session, processed f32le and consumption acknowledgements; one track, finite queue or optional interactive seek/edit/checkpoint resume |

## Read routes

Every route in this table supports `GET` and `HEAD`. Prefix paths with `/api/v1`. The new navigation and inspection routes reject unsupported, duplicate or incompatible parameters instead of ignoring them. The original `/status` and `/library` routes do not interpret query parameters.

A **page** is `{items,total,offset,limit,scanned_at}`. Pagination defaults to `offset=0&limit=50`, with `limit` from 1 to 200. Counts are calculated before slicing. Omit unsupported options rather than passing CLI flags verbatim.

| Route | Parameters besides pagination | Result |
| --- | --- | --- |
| `/status` | None; no pagination. | Server/API availability. |
| `/library` | None; no pagination. | Catalog date and entity counts. |
| `/albums` | `q` or `name`, `artist`, `year`, `genre`, `label`, `mbid`, `sort`, `order`. | Page of album summaries. |
| `/album` | Exactly one of `ref` or `name`; no pagination. | One album, its track references, artist, group and labels. |
| `/artists` | `q`, `mbid`, `sort`, `order`. | Page of artists and aliases. |
| `/artist` | Exactly one of `ref` or `name`; no pagination. | One artist, album references and attributed origin. |
| `/from` | Exactly one artist `ref` or `name`; no pagination. | `{reference,name,origin}`; explicit unknown origin when unavailable. |
| `/tracks` | `q`, `release` (stable reference), `sort`, `order`. | Page of track summaries. |
| `/track` | Exactly one of `ref` or `name`; no pagination. | One track, recording/album references, duration, local file facts and attributed analyses (including complete source data when available). |
| `/lyrics` | Required stable `track` reference; no pagination. | Complete local lyrics, source and optional line timestamps, or explicit `lyrics: null`. |
| `/genres` | `q` or `name`, `sort`, `order`. | Page of `{kind,reference,name,release_count,track_count}`. |
| `/genre` | Exactly one of `ref` or `name`; no pagination. | One genre and its linked albums/tracks, including a track's inherited album genre. |
| `/labels` | `q` or `name`, `mbid`, `sort`, `order`. | Page of `{kind,reference,name,mbid,release_count}`. |
| `/label` | Exactly one of `ref` or `name`; no pagination. | One label and its albums. |
| `/recordings` | `q`, `work` (stable reference), `sort`, `order`. | Page of recording summaries. |
| `/recording` | Exactly one of `ref` or `name`; no pagination. | One recording, its local tracks and works. |
| `/works` | `q` or `name`, `mbid`, `sort`, `order`. | Page of `{kind,reference,title,mbid,recording_count}`. |
| `/work` | Exactly one of `ref` or `name`; no pagination. | One work and its recordings. |
| `/release-groups` | `q` or `name`, `mbid`, `sort`, `order`. | Page of `{kind,reference,title,mbid,release_count}`. |
| `/release-group` | Exactly one of `ref` or `name`; no pagination. | One album identity and its local editions. |
| `/releases` | `q`, `artist` (stable reference), `year`, `sort`, `order`. | Original v1 album listing, preserved unchanged. |
| `/entities` | `ref` only; no pagination. | Original v1 generic entity detail, preserved unchanged. |
| `/doctor` | `severity=error|warning|info`. | Paged diagnostics, summary, unverified-file and pending-analysis counts. |
| `/stats` | None. | Library metrics; pagination applies independently to every breakdown/top table. |
| `/roots` | None. | Page of watched/excluded/unwatched folders and whole-library totals. |
| `/roles` | None. | Page of `{role,artists,credits}` actually present in the catalog. |
| `/countries` | Optional `q` (name, ISO code or derived initials). | Page of sourced artist countries/areas with coverage information. |
| `/years` | None. | Page of year totals, plus count of albums without a year. |
| `/search` | Required `q`; optional `comments=true|false` (default false). | Ranked names, optionally followed by matching stored tag comments. |
| `/query` | Required `q` (CLI expression); optional `sort`. | Page of matching tracks, using the core query engine. |

### Selection, filters and sorting

Use a stable token from a previous response with `ref`; URL-encode it. Singular routes also accept `name`: an exact normalized match takes precedence over partial matches. An ambiguous selection returns `409 ambiguous_entity` with `error.candidates: [{reference,name}]` (at most 200), rather than choosing an arbitrary album or artist. No match returns `404 entity_not_found`. To retrieve a specific duplicate title, repeat with its `ref`.

Artist selection recognizes stored aliases. Album, recording, work and release-group selection also recognizes a MusicBrainz identifier supplied through `name`. These navigation routes resolve catalog entities; a fetched work claim not represented as a local catalog work is not promoted to one by `/work` or `/works`. Each detail has the same entity fields as `/entities`, with `origin` added on `/artist`; it does not include every panel or presentation option of the CLI page.

On `/albums`, `artist`, `genre` and `label` accept a partial name or a stable reference of the right kind. Artist means album artist; genres consider both album and track genre links. Filters combine with AND. `year` is an exact unsigned 32-bit integer and `mbid` is exact and case-sensitive. `q` and `name` are alternatives: supply at most one. They search normalized names/titles by substring (case/accent insensitive). A filter naming an unknown artist/genre/label is an error.

For the browse lists, `sort=catalog` and `order=asc` are the defaults. Additional sorts: `name` on artists/genres/labels; `title` on releases/tracks/recordings; `name|title|year` on albums; `name|title` on works/release-groups. On albums, works and release-groups, `name` and `title` are aliases for sorting the title. `order=desc` changes the primary sort direction; equal normalized text keys keep catalog order. Missing album years stay last in either direction, with equal years ordered by normalized title, then catalog order. Countries keep the core's artist-count/name order; years are chronological. These two routes do not accept a sort override.

### Artist origin

`/artist` and `/from` read already stored MusicBrainz facts, never a release's pressing country and never an automatic lookup. The origin is:

```json
{
  "status": "unknown",
  "country_code": null,
  "area": null,
  "message": "No MusicBrainz information has been fetched for this artist; GET never fetches it automatically.",
  "attribution": null
}
```

The message is explanatory, not a stable code. Origins use `status: "known"` when at least one of country code or area is supplied; the other field may still be null. Attribution is `{source,source_id,fetched_at,confidence,match_score,trusted}`. Confidence is `identified` or `matched`; `match_score` is null for an identified lookup. Accepting a matched identity through the CLI can make it trusted without changing its original confidence or score. Untrusted matches and conflicting identities do not establish an origin. With neither country nor area available, or no fetch yet, the origin is explicitly unknown. A place may be a region rather than a country; `/countries` keeps official `iso_code` separate from `derived_initials`, which Aède derives for navigation.

### Diagnostics and search

`/doctor` reads current catalog/conclusions and source claims under the shared data lock, including integrity results saved since the last scan. A busy writer returns `409 store_busy`. It reports findings without running checks or fixing anything. Each issue has `type,label,severity,detail,files,file_count,files_truncated`; `files` is a preview of at most 20 paths. The summary counts all matching issues before pagination.

`/stats` includes counts, `duration_ms`, `bytes`, `tracks_without_album`, completeness ratios and paginated `by_codec,by_quality,by_sample_rate,by_decade,by_country,roles,top.artists,top.writers`. Country bucket counts mean artists, not tracks; their byte values are zero (not measured). `/roots` rows contain `status,path,tracks,duration_ms,bytes`; the synthetic `unwatched` row has a null path. Overlapping roots can overlap in row counts; `totals` counts each track once.

`/search` rows contain `kind,reference,name,context,found_in`. A track matching both its name and comment can have two hits distinguished by `found_in`. `/query` uses the [CLI grammar](../../docs/querying.md) and public sorts; personal clauses/sorts (notes, ratings, loves, tags, play history) and lyrics are explicitly refused, not evaluated with another user's data. Catalog search/name/identifier text is limited to 2048 UTF-8 bytes, stable references to 16384 bytes, and query complexity to 64 terms/parentheses/negations. Catalog listing/detail, navigation, inspection and personal work share two blocking-worker slots; catalog saturation returns `429 inspection_busy`. The constant-time `/status` and `/library` routes remain outside those workers.

### Local lyrics and client synchronization

```sh
curl --get 'http://127.0.0.1:3412/api/v1/lyrics' --data-urlencode 'track=track:…'
```

Copy the stable track token from the catalog; supply the same bearer session as other catalog reads when accounts exist. User, administrator and auditor sessions may read lyrics. The route never fetches or writes them, and does not add any messages to the PCM stream or methods to the Subsonic adapter.

The response is `{track,lyrics:null}` when no nonempty local lyrics exist, otherwise `{track,lyrics:{source:"tag"|"sidecar",synced:bool,lines:[{at_ms:u64|null,text:string}]}}`. A nonempty tag precedes the catalogued adjacent `.lrc`; `source` identifies this choice without an extra filesystem origin. All lines remain in source order, including repeated timestamps, untimed verses and timed blanks. `synced` means at least one timed line. Times are milliseconds from track start, with LRC offset already applied. For highlighting, group equal timestamps, sort groups chronologically and select the latest group at or before the **client's consumed playback position**. An empty timed group clears the preceding words. Pause freezes the position; seeking/repetition recalculates it. Untimed lyrics remain a readable page. See [playback](../../docs/server/playback.md#lyrics-and-the-client-clock) and the optional [client requirements](../../docs/server/compatible-aede.md#optional-synchronized-lyrics).

Complete reads require current audio size and precise timestamps; old catalogs need a normal rescan. Sidecars must match the audio basename and parent, with `.lrc` matched case-insensitively. Links/nonregular files and changed descriptors/paths are refused. Input is limited to 256 KiB, complete expanded text and serialized JSON independently to 1 MiB; excessive content returns `413 lyrics_too_large` instead of a prefix. Source errors are `404 source_unavailable`, `409 source_changed`, `404 lyrics_unavailable`, `409 lyrics_changed` or `500 lyrics_read_failed`. The shared worker budget applies, with a ten-second response deadline (`503 lyrics_timeout`); a timed-out blocking read retains its permit until it ends. Access is checked again before publication, and responses are private `no-store` data.

Corrupt source stores produce an error, never a false “no information” answer. Source-based reads may combine the latest atomic source file with the cached catalog; there is no multi-request snapshot guarantee.

## WebSockets

| Method and route | Purpose |
| --- | --- |
| Upgrade `GET /api/v1/events` | Initial catalog snapshot, then catalog changes only (original contract). |
| Upgrade `GET /api/v1/activity` | Catalog events plus task start/progress/completion/failure. |

Neither accepts commands. Notifications are best effort with no replay. Asynchronous HTTP jobs and delegated CLI jobs emit `running` and `refreshing` lifecycle progress with an explicit unknown work total (`done: 0, total: 0`); the legacy synchronous HTTP scan also reports discovery/read progress. Reconnect and poll an HTTP job's status to recover its result. See the [event schemas](../../docs/api.md#activity-stream).

## Administrative routes

Available to an administrator account session, or the optional `AEDE_ADMIN_TOKEN` (at least 32 ASCII characters, configured before startup). Without either, administration is disabled. Requests require one `Authorization: Bearer <token>` header. The legacy administrative token rejects every `Origin`; account sessions follow the shared local Origin check. Never use URL tokens. These routes stay local-only. The [account API reference](../../docs/server/accounts.md) lists login, password changes, account administration, limits and errors.

| Method and route | Body | Result |
| --- | --- | --- |
| `POST /api/admin/v1/scan` | No body. | Compatibility mode: synchronous rescan of watched roots; `200 {status:"completed",scanned_at,files}`. |
| `POST /api/admin/v1/scan` | JSON object, including `{}`. | Asynchronous parameterized scan; `202 {task_id,status:"queued",status_url}`. |
| `POST /api/admin/v1/fetch` | JSON object, including `{}`. | Asynchronous fetch with the same pass selection as the CLI; same 202 response. |
| `GET /api/admin/v1/tasks/{id}` | None. | Current task status and bounded result (also supports HEAD). |
| `POST /api/admin/v1/tasks/{id}/cancel` | No body. | `202 {task_id,status:"cancel_requested",status_url}`. |
| `GET /api/admin/v1/annotation?ref=<token>` | None. | The local owner's annotation, or `annotation: null` when nothing was written. |
| `PUT /api/admin/v1/annotation?ref=<token>` | Annotation patch. | Replaces supplied personal fields atomically and returns the resulting annotation. |
| `GET /api/admin/v1/history` | None. | Paged local-owner listening history, newest first. |
| `POST /api/admin/v1/history` | One play event. | Records one local-owner listening event; `201` plus its updated all-time count. |
| `GET /api/admin/v1/collection?name=<name>` | None. | One saved smart collection. |
| `PUT /api/admin/v1/collection?name=<name>` | `{ "expression": "…" }`. | Creates or replaces a saved smart collection. |
| `DELETE /api/admin/v1/collection?name=<name>` | No body. | Deletes that saved collection; `204`. |
| `GET /api/admin/v1/collections` | None. | Paged local-owner smart collections. |

Administrative task routes accept no query parameters; personal routes use only the query parameters shown above. JSON bodies are limited to 16 KiB and must finish within one second. Unknown fields/types return `400 invalid_body`; invalid option combinations return `400 invalid_parameters`. HTTP task IDs do not select delegated CLI jobs or compatibility-mode scans: those retain their existing behavior.

### Scan parameters

`folders: string[]` defaults to no new folders (rescan existing roots). Supplied folders are added to watched roots; `replace: true` instead replaces the watched list and requires at least one folder. Paths are absolute **server-side** paths, not phone/client paths, with at most 64 paths of 4096 bytes each. The token authorizes the server account's filesystem access; this is not a sandbox for untrusted callers.

Other fields: `full: bool`, `threads: integer` (0–64; 0 chooses automatically), `follow_symlinks: bool`, `include_hidden: bool`. Booleans default to false. A full scan rereads tags while keeping saved root exclusions and independent conclusions.

### Fetch parameters

All booleans default to false. `targets: string[]` contains artist/album names or server-side folders, just like positional CLI arguments (at most 64 nonempty strings, 4096 bytes each). Use absolute paths for unambiguous folder selection. The server's environment supplies external-service keys; a request cannot override keys, the data folder or executable arguments.

Pass flags: `summaries,discography,covers,lyrics,identify,credits,recordings,portraits,logos,labels,fanart`. With no pass flags, `{}` runs the ordinary CLI MusicBrainz identification pass; it is **not** a no-op. Selection and dependencies follow [fetch help](../../docs/sources.md).

Additional options:

- `full,dry_run,yes`: repeat stored lookups, preview without external requests, or explicitly accept the CLI's large-run confirmation.
- `lang: string`: the CLI's prose-language selection.
- `images: bool` and `size: string` require `covers`; sizes are `250|500|1200|original` (`full` is an alias for original, as in the CLI).
- `banners: bool` requires `logos` or `fanart`.
- `no_logo,no_label_logo,no_portrait,no_background,no_banner,no_album_cover,no_cdart` require `fanart`. Do not combine `logos` with `no_logo` or `banners` with `no_banner`.

Fetching is noninteractive: without `yes`, a run needing confirmation fails rather than waiting for input. Existing CLI rate limits, saved progress and non-overwrite rules apply; fetched images/lyrics may create derivative files beside music. Audio files and tags remain untouched.

```sh
# AEDE_ADMIN_TOKEN here must match the secret configured in the running server.
curl -X POST 'http://127.0.0.1:3412/api/admin/v1/scan' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' -d '{}'

curl -X POST 'http://127.0.0.1:3412/api/admin/v1/fetch' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"targets":["Miles Davis"],"summaries":true,"lang":"fr","dry_run":true}'
```

### Follow and cancel work

Status shape:

```json
{
  "task_id": 1,
  "task_kind": "scan",
  "status": "completed",
  "cancel_requested": false,
  "result": {"exit_code": 0, "stdout": "...", "stderr": "", "output_truncated": false},
  "error": null
}
```

Possible statuses: `queued,running,completed,failed,cancelled`; task kinds: `scan,identification` (fetch). `result` is null until available, and each output stream is limited to 64 KiB. Failures include `error: {code,message}`. There are at most four active HTTP jobs and 64 retained records; older terminal records are evicted. Full capacity returns `503 task_limit`; missing/expired IDs return `404 task_not_found`; cancelling a finished job returns `409 task_not_cancellable`.

Accepted jobs survive client disconnection and wait for the shared writer lock. Cancelling requests a stop; completed saves are not rolled back. Graceful shutdown waits for accepted jobs, so cancel a long fetch first if it should not finish. IDs/results are in memory only and disappear on restart; polling and retrying a lost submission is not idempotent. Never blindly resubmit after a connection failure.

### Personal data: annotations, plays and smart collections

These transitional administrative routes always address `local`, using administrator authentication. Account sessions use the identical selectors, bodies and response shapes at `/api/me/v1/{annotation,history,collection,collections}`, bound to their own stable owner. Requests cannot choose an owner. Private values remain separate from catalog responses, and `user.json` keeps its existing format. Session checks are repeated under the data lock; personal workers are bounded and shared with inspection (`503 personal_busy` when occupied).

`PUT /annotation?ref=<token>` accepts one or more of the following fields. Omitting a field leaves it unchanged; an explicit `null` removes `rating`, `note`, or all `tags`. `loved` must be a boolean. Rating is 1–5. Notes must contain non-whitespace text. Tags replace the complete set, are sorted in the response, and allow at most 100 unique non-empty entries of 256 bytes. A patch which leaves no favourite, rating, note or tag removes the stored annotation rather than keeping an empty record.

```json
{
  "loved": true,
  "rating": 5,
  "note": "Original pressing; compare the remaster.",
  "tags": ["vinyl", "reference"]
}
```

The reference must name a current catalog entity and contain at most 16384 UTF-8 bytes. The response is `{reference,annotation}`, where `annotation` is `null` when no values remain. This API never writes audio tags or audio files.

`POST /history` accepts `{track,at?,ms_played,completed}`. `track` is a current track reference; `at` defaults to the server time and cannot be over one day in the future; `ms_played` is capped at 24 hours. Each accepted event updates the bounded recent-history log and the all-time per-track count together. Delayed events are ordered by `at`, not receipt time: the log retains the newest events by date, while every accepted event still increments its all-time count. Existing arrival-ordered history is sorted when loaded without dropping events or changing counts. `GET /history?offset=&limit=` returns `{items,total,offset,limit,scanned_at}` with each item containing `track,at,ms_played,completed,play_count,last_played`. Sorting happens before pagination, newest first; equal-time events remain distinct and the last received appears first.

A collection stores a **query expression**, not a static list of tracks: its result evolves with the catalog. `PUT /collection?name=<name>` validates the current Aède query grammar before saving a non-empty expression of up to 2048 bytes. Names are matched case/accent-insensitively, like the CLI, and must contain non-whitespace text within 256 UTF-8 bytes for GET, PUT and DELETE. `GET /collections` uses normal `offset`/`limit` pagination.

Every personal read and write obtains the same data-directory lock as the CLI and loads both the current on-disk catalog and `user.json` while holding it. Target validation and reconciliation therefore use the same completed catalog version, even before the public catalog cache refreshes after a scan. The lock remains held through an update's atomic save to `user.json`; a competing request receives `409 store_busy` and can retry. An absent catalog returns `503 catalog_unavailable`, an unreadable catalog returns `500 store_error`, and failed/corrupt user-data reads return `500 user_unavailable`; no personal data is saved on these failures. Unexpected background failures return `500 personal_failed`.

## Deliberately not exposed yet

The native HTTP routes leave static-playlist creation/export, relation annotations, source-review decisions, arbitrary file inspection, check/analyze/fingerprint, copy, backup/restore, reset, merge and generic command execution to other interfaces. The [Subsonic adapter](../../docs/server/subsonic.md) exposes private persistent static playlists and verified sidecar artwork under its compatibility contract. Aède's `playlist` CLI command generates an M3U from a current selection, while native collection routes retain query collections. CLI availability does not imply an HTTP route. Native prose/artwork delivery still needs a defined contract; local lyrics use `/lyrics`, and native audio uses the documented PCM WebSocket. CLI-only arguments such as `--json`, `--csv`, `--output`, personal filters and unsupported presentation switches are rejected by HTTP.

## Code layout

`lib.rs` contains module wiring and the public entry points, not all server behavior:

- `routing.rs`: route registration.
- `runtime.rs`, `state.rs`: startup/shutdown, catalog reloads and shared state.
- `catalog.rs`, `catalog_commands.rs`, `inspection.rs`: original catalog contract, CLI-shaped navigation, diagnostics/search.
- `lyrics.rs`: complete bounded local lyrics, independent of PCM transport.
- `models.rs`, `query.rs`, `errors.rs`: JSON types, original list validation and error envelopes.
- `security.rs`, `auth.rs`, `accounts_api.rs`, `tls.rs`: Host/Origin checks, account authentication, account routes and HTTPS configuration.
- `events.rs`: WebSocket notifications.
- `playback_api.rs`: authenticated PCM transport and client acknowledgements.
- `playback_interactive.rs`: opt-in epoch/reset controls and private queue/settings/position checkpoint publication. These checkpoints are resumable cursors, not extra listening events; the [wire guide](../../docs/server/playback.md#interactive-queues-seeking-and-persistent-resume) defines source validation, client buffer discard, explicit resume and durability limits.
- `admin.rs`, `jobs.rs`, `personal.rs`: compatibility scan, asynchronous HTTP work and owner-scoped personal data.
- `delegation.rs`: the Unix-only local CLI channel, separate from HTTP tasks.
- `subsonic/`: compatibility parsing/envelopes, API-key checks, ID3/private-data projection, bounded original audio/artwork and temporary owner-scoped now-playing reports.
- `*_tests.rs`, `test_support.rs`: isolated tests and shared test-only fixtures.

The CLI adapter in `aede-cli/src/commands/server_jobs.rs` maps typed requests to existing commands without a shell. No new storage format or database is required.

## M4 device controller

The public `devices` path reexports the standalone [`aede-devices`](../aede-devices/README.md) crate for existing Rust callers. The CLI calls that crate directly for explicit finite SlimProto, UPnP AVTransport and OpenHome Playlist control. It does not add a route to the authenticated catalog API. A capability/peer restricted temporary media listener serves only selected unchanged originals. Subsonic and the device transport share MIME/suffix/byte-range policy through `aede_devices::original`, while account authentication and revocation stay here. See [Network audio devices](../../docs/server/devices.md) for protocol profiles, LAN restrictions and outstanding hardware acceptance.
