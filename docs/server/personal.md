# Personal annotations, history and collections

These routes require [administrative authentication](administration.md) for reads **and** writes, refuse Origin and always address the existing `local` owner. They do not provide multiple accounts. They read the current on-disk catalog and `user.json` under the shared lock, so selection and atomic updates use the same completed catalog version.

## GET /api/admin/v1/annotation

Required `ref`, naming a current catalog entity; no body or pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/admin/v1/annotation' \
  --data-urlencode 'ref=artist:REFERENCE_FROM_RESPONSE' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Replace the illustrative token with a response's actual reference. Returns HTTP 200 `{reference,annotation}`. Annotation is null if no personal values were written; otherwise its fields are `reference,loved,rating,note,tags,created_at,updated_at`. The two dates use Unix seconds; absent rating/note are null and tags is an array. Public `/artist` or `/track` reads never include these private values. Invalid reference gives 400; a valid entity absent from the current catalog gives 404.

### HEAD /api/admin/v1/annotation

Same token/reference/lock validation without an annotation body.

## PUT /api/admin/v1/annotation

Required `ref` query selector; JSON patch with one or more of these fields:

| Field | Accepted value | Meaning |
| --- | --- | --- |
| `loved` | boolean true/false | Set/remove favourite; null is invalid. |
| `rating` | integer 1–5 or null | Stars; null removes the rating. |
| `note` | text containing non-whitespace, or null | Personal Markdown text; null removes it. It is stored text, not executable HTML. |
| `tags` | string array or null | Replace the **entire** tag set; null/empty array clears it. At most 100 unique nonempty tags, each 256 bytes, no NUL; duplicates are refused. |

Omitting a field leaves it unchanged. Empty `{}` is refused. Tag order in the response is sorted. A resulting annotation with no favourite/rating/note/tag is removed rather than retaining an empty record. No audio tags/files are edited.

Keep selector and JSON body separate: curl's `--get` can move data into the query. Use a URL with an already URL-encoded reference plus `-X PUT -d 'JSON'`, or a client that separately specifies query and JSON. After assigning a URL-encoded token from a previous response to `AEDE_REF_URL`:

```sh
curl -X PUT "http://127.0.0.1:8787/api/admin/v1/annotation?ref=$AEDE_REF_URL" \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' -d '{"loved":true,"rating":5}'
```

Returns HTTP 200 with the updated `{reference,annotation}`. Wrong field types/unknown fields give `invalid_body`; invalid ranges, null loved, empty notes or invalid tags give `invalid_parameters`. Store conflicts/errors below apply before any write.

## GET /api/admin/v1/history

Parameters: `offset`, `limit`; no body. Lists recent listening events newest first.

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/history?limit=20' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 200 with a page; items contain `track,at,ms_played,completed,play_count,last_played`. Times `at`/`last_played` are Unix seconds; played duration is milliseconds. Counts describe the all-time track total, not just this page. History is bounded while per-track counts survive older events dropping from the recent log. Events are ordered by date; equal-date events remain distinct, with the last received first.

### HEAD /api/admin/v1/history

Same authenticated page/lock validation without history JSON.

## POST /api/admin/v1/history

No query parameters. JSON fields: required `track` (current track reference), `ms_played` (unsigned integer, max 24 hours = 86400000), `completed` (boolean); optional `at` (Unix seconds, defaults to server time, not over one day in the future).

```sh
curl -X POST 'http://127.0.0.1:8787/api/admin/v1/history' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"track":"track:REFERENCE_FROM_RESPONSE","ms_played":180000,"completed":true}'
```

Replace the illustrative track token. Returns HTTP 201 with one event view using the fields above and updated all-time count. This **records an event supplied by the client**; it does not play or verify listening. Every accepted POST increments its count, including delayed events outside the newest retained log. It is not idempotent: repeating a lost submission can double-count. Wrong-kind/malformed track gives 400, absent track 404, duration/date bounds 400. No audio streaming route exists behind this endpoint.

## GET /api/admin/v1/collection

Required nonempty `name`, no body/pagination. Names match case/accent-insensitively.

```sh
curl --get 'http://127.0.0.1:8787/api/admin/v1/collection' \
  --data-urlencode 'name=Favourites' -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 200 `{name,expression,created_at,updated_at}`. Timestamps are Unix seconds. This returns the saved **query expression**, not its current track result. An unknown name gives `404 collection_not_found`.

### HEAD /api/admin/v1/collection

Same name/token validation and status, no collection body.

## PUT /api/admin/v1/collection

Required `name` (1–256 bytes); JSON `{expression:"…"}` with 1–2048 bytes of nonempty expression. The grammar is validated before saving. Existing normalized names are replaced; new names are created. A smart collection evolves as library/personal values change, unlike a static playlist.

```sh
curl -X PUT 'http://127.0.0.1:8787/api/admin/v1/collection?name=Favourites' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' -d '{"expression":"loved"}'
```

Returns HTTP 200 `{name,expression,created_at,updated_at}`. Invalid expression/size gives `400 invalid_parameters`, invalid name `invalid_query`, wrong/unknown body fields `invalid_body`. Personal clauses are allowed in this owner's stored expression; public `/query` still refuses them. Saving does not export M3U or create a playback task.

## DELETE /api/admin/v1/collection

Required `name`, no body. Deliberately removes that saved collection, leaving music and annotations untouched.

```sh
curl -X DELETE 'http://127.0.0.1:8787/api/admin/v1/collection?name=Favourites' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 204 with no body; missing collection gives `404 collection_not_found`. This is an actual removal, not a preview.

## GET /api/admin/v1/collections

Only `offset`, `limit`, no body.

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/collections?limit=20' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

Returns HTTP 200 with a page of `{name,expression,created_at,updated_at}`. An empty list means no saved collections. Unsupported filters give 400; no expression is executed by listing it.

### HEAD /api/admin/v1/collections

Same authenticated pagination/store validation, without collection JSON.

## Shared failures and recovery

All these reads/writes obtain the data-folder lock. Another writer gives `409 store_busy`; retry once it finishes. Absent catalog: `503 catalog_unavailable`; unreadable catalog: `500 store_error`; corrupt/unreadable user data: `500 user_unavailable`; unexpected worker failure: `500 personal_failed`. No personal save occurs on those load/validation failures. Preserve damaged stores and backups before recovery instead of resetting data. [Backup operation](../operating.md#backups-and-recovery) covers safe recovery.

Static playlist creation/export, relation annotations, source review, check/analyze/fingerprint, copy, backup/restore, reset, merge, arbitrary files and shell commands have no current HTTP routes. Lyrics/prose, binary artwork and audio delivery need separate contracts. Their CLI availability does not imply API availability.
