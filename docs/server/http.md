# Understand HTTP and JSON

An API is an interface a program can call. Aède's local API exposes data rather than a rendered player. The examples assume a server listening at `http://127.0.0.1:8787`; change the port if your startup message says otherwise.

## Read a URL

`http://127.0.0.1:8787/api/v1/albums?limit=10` contains the base address, a route (`/api/v1/albums`) and a query parameter (`limit=10`). Join several parameters with `&`. Use `curl --get --data-urlencode` for values containing spaces, accents, slashes or reference tokens:

```sh
curl --get 'http://127.0.0.1:8787/api/v1/album' \
  --data-urlencode 'name=Back in Black'
```

`GET` reads data. `HEAD` returns the same status/headers without a body; `curl -I URL` issues HEAD. Catalog routes support both. `POST` submits an operation, `PUT` updates, `PATCH` changes selected account fields and `DELETE` removes/revokes. Mutations exist only on documented administrative, authentication and personal routes. CLI options such as `--json`, `--output` or `--csv` are not HTTP parameters.

## Decode a page

List endpoints return this envelope; values below illustrate an empty catalog page:

```json
{"items":[],"total":0,"offset":0,"limit":50,"scanned_at":0}
```

`items` contains the current page. `total` counts all matches before slicing. `offset` is the number of matches skipped, starting at 0. `limit` is the largest requested page length: default 50, allowed 1–200. For page two with a length of 50, use `offset=50&limit=50`. A page beyond the end is a successful empty page. Search/filtering happen first, sorting second and pagination last.

`scanned_at` is the scan time in Unix seconds. Compare it between pages; if it changes, restart paging when you need a consistent view. There is no snapshot spanning multiple requests. Durations ending in `_ms` use milliseconds and sizes use bytes. `null` means a fact is unavailable; an empty array means there are no listed relationships. Names preserve local spelling. Ignore response fields you do not recognize, so additive API upgrades remain compatible.

## Choose an entity

An **entity** is an artist, release, track, recording, work, release group, label or genre. An album is a release; a track places a recording inside that release. Responses use a stable `reference` token to link entities. Copy that token as received and pass it through `--data-urlencode`; do not invent catalog array indexes. Track references are path-based and change if the file moves.

Singular routes accept exactly one of `ref` or `name`. Names compare without case/accent sensitivity; an exact normalized match wins over partial matches. Artist aliases are recognized. Album, recording, work and release-group selectors also recognize a MusicBrainz identifier passed as `name`.

If several entities match, Aède returns `409 ambiguous_entity` and up to 200 `{reference,name}` candidates in `error.candidates`. Repeat the request with the desired candidate's `ref`. No match returns `404 entity_not_found`. Supplying both selectors, neither selector, a wrong-kind reference or unsupported parameters is an error.

## Filters and order

Filters combine with AND. Ordinary browse lists default to `sort=catalog&order=asc`; catalog order is the deterministic saved scan order. Each route specifies its accepted alternatives. Text ties retain catalog order. Missing album years remain last in either direction; equal years are ordered by normalized title, then catalog order. Exact `mbid` filters are case-sensitive. Countries and years have fixed ordering and refuse sort overrides.

Unknown/duplicate parameters and unsupported combinations are refused. `/status` and `/library` are older exceptions that do not interpret query parameters; omit parameters on both rather than relying on that exception.

Catalog `q`, `name` and `mbid` values accept at most 2048 UTF-8 bytes after URL decoding and before normalization. Stable references, including `ref` and reference filters, accept at most 16384 UTF-8 bytes. Selectors that accept either a name or a reference apply the 2048-byte limit to names and the 16384-byte limit to recognized reference-kind prefixes; references must still have the kind required by the route. Oversized values return `400 invalid_query`.

Catalog lists and entity details, including the original `/artists`, `/releases`, `/tracks`, `/recordings` and `/entities`, share two blocking workers with navigation, inspection and personal operations. Catalog/navigation/inspection saturation returns `429 inspection_busy`; personal saturation returns `503 personal_busy`. `/status` and `/library` read snapshot metadata and counts in constant time outside this worker budget. Authentication and transport admission still apply.

## Read errors

```json
{"error":{"code":"entity_not_found","message":"no entity matches this reference"}}
```

The HTTP status gives the broad outcome; `error.code` is the stable value software should inspect. The human explanation in `message` may change.

| Status / code | What to do |
| --- | --- |
| 400 `invalid_query`, `invalid_parameters`, `invalid_reference` | Check the route's parameter names, selector and value types. |
| 400 `invalid_pagination` | Use unsigned integers and a limit from 1 to 200. |
| 403 `invalid_host` / `invalid_origin` | Use the permitted local address/port or configured HTTPS authority and matching browser origin. |
| 404 `entity_not_found` / `not_found` | Refresh the reference, or check the URL. |
| 405 `method_not_allowed` | The route does not support this HTTP method. |
| 409 `ambiguous_entity` | Pick one returned reference. |
| 409 `store_busy` | Wait for the current writer and retry a read; do not remove its lock. |
| 429 `inspection_busy` | The two shared catalog/navigation/inspection/personal workers are occupied; retry later. |
| 500 `sources_unavailable`, `catalog_read_failed`, `inspection_failed`, `store_error` | Inspect the server's error log and preserve data before recovery. |
| 503 `catalog_unavailable` | The catalog was removed/unavailable; restore it or scan, then retry. |

Administrative authentication, body validation and task-specific errors are explained in [Administration](administration.md), [Jobs](jobs.md) and [Personal data](personal.md). Transport/parser failures and WebSocket handshake errors do not guarantee this JSON envelope.

## Local access boundary

With no [account store](accounts.md) on first access in a fresh local HTTP process, catalog reads need no token and may reveal music paths, comments and names to other processes/users on this computer. Once the process has observed the store, catalog reads require a bearer session or, on local HTTP, administrative token; a missing or unreadable store then fails closed until the process stops. Restarting local HTTP without the store restores anonymous compatibility mode. An unreadable store fails closed even on first access. [HTTPS](remote.md) always requires accounts, accepts only sessions and validates Host and supplied Origin against its configured authority. HTTP stays restricted to loopback. Neither transport grants cross-origin permission.

Never embed a secret in a web page or URL. Legacy administrative-token requests refuse every Origin header; account sessions follow the transport's same-origin check. A future website/player still needs its own login/cookie design.

## Route map

| Need | Read next |
| --- | --- |
| Albums, artists, origin, tracks and performances | [Catalog routes](catalog.md). |
| Genres, labels, compositions and edition groups | [Graph routes](graph.md). |
| Availability, totals, diagnosis, roots, roles, countries, years and original compatibility routes | [Inspection routes](inspection.md). |
| Ranked names and public track expressions | [Search/query](search.md). |
| Catalog notifications and task lifecycle | [WebSocket streams](events.md). |
| Enable the optional token boundary | [Administration](administration.md). |
| Submit/follow/cancel scan or fetch | [HTTP jobs](jobs.md). |
| Owner annotations, history and collections | [Personal routes](personal.md). |
| Configure an authenticated remote listener | [HTTPS](remote.md). |
| Stream processed audio and acknowledge playback | [Audio contract](playback.md). |

The [versioned API contract](../api.md) specifies compatibility and exact field types; these guides explain how to use the implemented interface.
