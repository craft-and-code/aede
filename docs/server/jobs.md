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
| `musicbrainz` | Identify artists/albums before the selected secondary passes, in the same server-owned task. Reuses stored answers unless `full` is true; preserves scope, dry-run and cancellation. Advertised as `fetch_musicbrainz_first`. An identification failure stops the chain; completed source answers remain stored. |
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
| `discogs` | Fetch identifier-linked Discogs label biographies explicitly; respects scope, dry-run and recent retained profiles. |
| `fanart` | Enable the Fanart.tv artwork families. |
| `banners` | Include banners; requires `logos` or `fanart`. |
| `images` | Download cover images; requires `covers`. |
| `size` | String `250`, `500`, `1200`, `original`; `full` aliases original. Requires `covers`. |
| `lang` | Nonempty prose-language string, max 256 bytes/no NUL; follows CLI language selection. |
| `full` | Repeat stored lookups instead of skipping already obtained results. |
| `dry_run` | Preview without external requests. |
| `yes` | Explicitly accept a CLI large-run confirmation. HTTP does not wait for interactive answers. |
| `no_logo,no_label_logo,no_portrait,no_background,no_banner,no_album_cover,no_cdart` | Boolean artwork-family exclusions; all require `fanart`. Do not pair `logos` with `no_logo` or `banners` with `no_banner`. |

Pass dependencies and selection behavior follow [the source guide](../sources.md). Service keys ordinarily come from the server environment. A authenticated administrator may supply `service_keys: {fanarttv?: string, acoustid?: string}` for this job only (nonempty strings, at most 1024 bytes, no control characters). Missing keys retain their server defaults. The server does not persist these client keys, include them in task metadata, or accept other environment variables, data folders or executable arguments. Status advertises `fetch_service_keys` when the administrative profile is available; HTTPS requires a named administrator session. The ordinary CLI callback passes overrides only in the child environment and redacts them from captured output. Existing rate limits, save-progress and non-overwrite rules remain active. Fetching images/lyrics can create derivative sidecars beside music; audio files/tags stay untouched. A noninteractive run that needs confirmation and lacks `yes` fails rather than prompting. Invalid flags/bounds give 400; an accepted operation can later fail in its task result.

Captured diagnostics mask raw, trimmed and percent-encoded service-key variants against the original text. Overlapping secrets and overlapping occurrences are covered by one merged redaction span; generated markers are never scanned as credentials. Truncated boundary fragments remain masked and each returned stream stays within 64 KiB. This is diagnostic protection, not encryption or a substitute for the authenticated local administrative boundary. Direct and Unix-delegated AcoustID identification use the same masking before showing provider or transport failures; retry, rate-limit and failure semantics stay intact.

## GET /api/admin/v1/tasks

Administrators can recover the running server's retained tasks after a client reconnect without replaying a job or guessing IDs. The `administrative_tasks_list` capability advertises the additive read. Returns an array with the same authenticated task fields as the singular endpoint, capped by the existing 64-record registry and bounded outputs. Ordinary users/anonymous callers are refused; HTTPS requires a named administrator session. Records are process-local: restarting an embedded/server engine clears the registry, while per-answer source data already written by fetch remains persisted. A new explicitly requested fetch reuses retained fresh profiles.

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

At most four HTTP jobs are active and 64 records retained; oldest finished records are evicted. Jobs survive client disconnection. Acoustic decoding runs without the shared writer lock; each album publication acquires it briefly, reloads the current catalogue and merges results. This preserves intervening scans and leaves account session publication available during the expensive calculations. Other store-changing commands keep their existing lock policy. Graceful shutdown waits for accepted jobs, so cancel first if a long fetch should stop. IDs/results are in memory only and disappear after restart.

A POST whose response was lost may already have started. Submissions are not idempotent; do not blindly repeat them. Reconnect/poll a saved ID and inspect activity/logs before deciding to submit again. Recovery does not rewind external requests or saved partial results.

## POST /api/admin/v1/lyrics

A authenticated administrator can submit `{track:"track:..."}`. The server validates the exact current track source and forwards only that file to the existing cancellable fetch worker with `lyrics:true,yes:true`. `fetch_track_lyrics` advertises the route. No arbitrary client filesystem path is accepted; album/artist references are refused. LRCLIB requires no API key in the existing Aède profile. Provider failures and missing matches remain task output, not invented words. Existing sidecars are preserved, audio/tags are untouched, and `/api/v1/lyrics` discovers a newly written adjacent LRC without requiring a catalog rescan. HTTPS requires a named administrator session.

Discogs batch enrichment does not stop at the first label without a usable provider link/profile. It reports each unavailable label, continues with remaining identified labels and preserves every completed profile atomically. A final partial-failure status retains diagnostics and previously saved biographies. Provider 429 or exhausted transient throttling still stops the pass immediately; continuing the batch never bypasses provider backoff/rate rules. Fresh completed profiles are reused by the next explicit run.

## POST /api/admin/v1/generate

A trusted local administrator can submit a whole-library derivative plan, advertised by `library_generations`:

```json
{"steps":["analyze","spectrum","m3u","lyrics"]}
```

`steps` contains one to four distinct values in the requested order. Unknown stages, duplicates, empty plans, arbitrary paths/options and other body fields are refused. Current watched roots are captured from the server catalog, never supplied by the client. A lyrics stage requires the fetch feature. The response uses the existing accepted-task shape and polling/cancellation routes. Single-stage task kinds retain `library_generation_analyze`, `library_generation_spectrum`, `library_generation_m3u` or `library_generation_lyrics`; compound work uses `library_generation`.

The existing CLI worker runs each stage successively: cached FlacCompagnon reports at album layout, cached spectrograms, canonical album-folder M3Us, then the explicit LRCLIB lyrics pass. Audio analysis and spectrum use one worker. The M3U stage does not derive an artist playlist outside a watched root. Shared derivative files remain distinct from owner-private saved queues and collections; original audio and tags are unchanged. Existing valid work is reused according to each command's normal policy. A completed stage with nonzero exit status is reported, later selected stages still run, and the final task remains failed rather than reporting false success. Cancellation stops the remaining stages; completed writes remain retained.

Generation reserves the administrative registry until termination is confirmed and refuses concurrent administrative work; another active job prevents admission. The task lives on Aède and survives closing the initiating client view or connection. Process records remain bounded and process-local as above. Unix children run in a separate process group and cancellation signals only that owned group, including nested encoders. Windows uses the system's owned-child tree termination command; Windows runtime acceptance remains separate. No arbitrary PID, executable or shell text is accepted from HTTP. The host regression covers inherited worker pipes and cancellation latency. One worker and serialized stages reduce resource competition; they do not certify uninterrupted physical playback under arbitrary operating-system load.


## POST /api/admin/v1/check

Album full decode test: `{"release":"release:…"}`. Authenticated administrators request a fresh, sequential decode of the exact catalogued files of one edition. Arbitrary paths, folder expansion and additional options are refused. The existing task read/cancel routes expose the real result; closing the view does not stop accepted work. `album_decode_test` advertises support. `album_decode_results` additionally advertises the versioned per-file JSON report (`version: 1`, exact `path`, `status: ok | no_hash | ko`, `verifier`, optional `message`). The worker uses `check --decode-test --json`: FLAC uses `flac -t` if available on the engine host, otherwise the existing Rust decoder; other codecs use the existing decoder. A failed external test never falls back to a success. Missing FLAC MD5 is distinct from a verified digest. Original audio, tags and persisted CRC/analysis conclusions are unchanged. Unreadable/failed files produce a failed task; read the per-file result. HTTPS requires a named administrator session.
