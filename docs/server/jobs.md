# Scan, fetch, follow and cancel jobs

Every request here uses [administrative authentication](administration.md). The examples assume `AEDE_ADMIN_TOKEN` contains the same private token as the running server. Paths in request bodies refer to the **server computer**. Task endpoints accept no query parameters.

## POST /api/admin/v1/scan — no body

Compatibility mode synchronously rescans existing watched roots. It keeps stored exclusions, artist merges and independent conclusions. The connection waits until the scan is saved and published.

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/scan' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 200 with `{status:"completed",scanned_at,files}`. The timestamp is Unix seconds, and files is the discovered catalog file count. This mode cannot add folders through a body and is not an asynchronously pollable HTTP job. Another writer gives `409 store_busy`; scan/storage failure gives `500 scan_failed` or `store_error`. Send `{}` to request the asynchronous mode instead.

## POST /api/admin/v1/scan — JSON object

An object, even empty, queues a typed asynchronous scan:

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/scan' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"folders":["/path/to/music"],"full":false,"threads":0}'
```

| Field | Type/default | Meaning |
| --- | --- | --- |
| `folders` | string array, empty | Add absolute server-side watched roots; empty rescans existing roots. At most 64 paths, 4096 bytes each, no NUL. |
| `replace` | boolean, false | Replace watched roots instead of adding; requires at least one folder. It is an explicit selection change, not audio deletion. |
| `full` | boolean, false | Reread tags even where an incremental scan could reuse data; preserves saved exclusions/conclusions. |
| `threads` | integer, automatic if omitted | 0 chooses automatically; accepted range 0–64. |
| `follow_symlinks` | boolean, false | Follow symbolic links during discovery. |
| `include_hidden` | boolean, false | Include hidden entries during discovery. |

Returns HTTP 202, for example `{"task_id":1,"status":"queued","status_url":"/api/admin/v1/tasks/1"}`. **202 means accepted, not completed**. Save the ID/URL and poll. Invalid paths/combinations return `400 invalid_parameters`; malformed JSON/unknown fields return `400 invalid_body`; capacity gives `503 task_limit`.

## POST /api/admin/v1/fetch

Requires a JSON object. Fetch is the explicitly requested online step: it obtains attributed information without replacing local tags.

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/fetch' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"targets":["Miles Davis"],"summaries":true,"lang":"fr","dry_run":true}'
```

This example previews the selected work without contacting external services. Remove `dry_run:true` only when you intend to perform it. Returns the same HTTP 202 queued-task shape as scan, with `task_kind:"identification"` in later status.

All booleans default to false; targets defaults to empty. **`{}` is not a no-op**: with no pass flags it runs the ordinary CLI MusicBrainz identification pass.

| Field(s) | Meaning / constraints |
| --- | --- |
| `targets` | At most 64 nonempty artist/album names or server-folder paths, 4096 bytes each, no NUL. Absolute paths make folder selection unambiguous. |
| `summaries` | Fetch artist/album prose through the CLI source pipeline. |
| `discography` | Fetch artist discography information for later missing-album inspection. |
| `covers` | Fetch cover-art information; derivative image downloads require `images`. |
| `lyrics` | Explicitly fetch lyrics, never enabled implicitly. |
| `identify` | Enable the CLI identification pass. |
| `credits` | Fetch album/performance/work credits through the existing scoped pipeline. |
| `recordings` | Fetch recording-level identification/information. |
| `portraits` | Fetch supported artist portraits. |
| `logos` | Fetch supported artist/label logos. |
| `labels` | Fetch label information. |
| `fanart` | Enable the Fanart.tv artwork families. |
| `banners` | Include banners; requires `logos` or `fanart`. |
| `images` | Download cover images; requires `covers`. |
| `size` | String `250`, `500`, `1200`, `original`; `full` aliases original. Requires `covers`. |
| `lang` | Nonempty prose-language string, max 256 bytes/no NUL; follows CLI language selection. |
| `full` | Repeat stored lookups instead of skipping already obtained results. |
| `dry_run` | Preview without external requests. |
| `yes` | Explicitly accept a CLI large-run confirmation. HTTP does not wait for interactive answers. |
| `no_logo,no_label_logo,no_portrait,no_background,no_banner,no_album_cover,no_cdart` | Boolean artwork-family exclusions; all require `fanart`. Do not pair `logos` with `no_logo` or `banners` with `no_banner`. |

Pass dependencies and selection behavior follow [the source guide](../sources.md). Service keys come from the server environment; clients cannot override keys, data folders or executable arguments. Existing rate limits, save-progress and non-overwrite rules remain active. Fetching images/lyrics can create derivative sidecars beside music; audio files/tags stay untouched. A noninteractive run that needs confirmation and lacks `yes` fails rather than prompting. Invalid flags/bounds give 400; an accepted operation can later fail in its task result.

## GET /api/admin/v1/tasks/{id}

Use the numeric ID from a JSON-submitted scan/fetch, not a catalog reference or a delegated CLI task ID.

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/tasks/1' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 200 with:

```json
{"task_id":1,"task_kind":"scan","status":"completed","cancel_requested":false,"result":{"exit_code":0,"stdout":"...","stderr":"","output_truncated":false},"error":null}
```

Statuses are `queued,running,completed,failed,cancelled`. Result is null until available; each stdout/stderr stream is limited to 64 KiB. `output_truncated:true` means output is incomplete. Failures contain `error:{code,message}`; exit code 0 indicates success. Poll with a sensible delay; [activity](events.md) announces lifecycle but is not a durable result store. Unknown/expired IDs give `404 task_not_found`; IDs must be positive decimal whole numbers (invalid values give `400 invalid_task_id`). An unexpected registry failure gives `500 task_error`. Delegated tasks and the bodyless compatibility scan are not selectable here.

### HEAD /api/admin/v1/tasks/{id}

Same authentication/ID validation; status/headers only, without the result. `curl -I URL -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"` checks existence.

## POST /api/admin/v1/tasks/{id}/cancel

No body and no query parameters.

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/tasks/1/cancel' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 202 `{task_id,status:"cancel_requested",status_url}`. Cancellation is a request to stop at a safe point, not proof it has already stopped. Poll until terminal status. Saved progress is **not rolled back**. A finished task gives `409 task_not_cancellable`; unknown/expired IDs give 404.

## Job lifetime and retry

At most four HTTP jobs are active and 64 records retained; oldest finished records are evicted. Jobs wait for the shared writer lock and survive client disconnection. Graceful shutdown waits for accepted jobs, so cancel first if a long fetch should stop. IDs/results are in memory only and disappear after restart.

A POST whose response was lost may already have started. Submissions are not idempotent; do not blindly repeat them. Reconnect/poll a saved ID and inspect activity/logs before deciding to submit again. Recovery does not rewind external requests or saved partial results.
