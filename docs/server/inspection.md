# Library inspection and compatibility routes

These are read-only views. They never check audio, fix tags, fetch data or perform a backup. [HTTP basics](http.md) defines common status codes and paging. All ordinary routes below support GET and HEAD.

## GET /api/v1/status

No interpreted parameters or pagination. Use it to check that the process is responding:

```sh
curl http://127.0.0.1:8787/api/v1/status
```

Returns `{"status":"ok","api_version":1,"catalog_loaded":true}` when a catalog is loaded. The status endpoint remains available if the catalog disappears; then `catalog_loaded` is false. `api_version` is the HTTP contract version, independent of the executable/store versions. A connection-refused error usually means the server is stopped or you chose the wrong port; it is not a JSON API error.

### HEAD /api/v1/status

`curl -I http://127.0.0.1:8787/api/v1/status` checks HTTP availability without the JSON body.

## GET /api/v1/library

No interpreted parameters or pagination.

```sh
curl http://127.0.0.1:8787/api/v1/library
```

Returns `scanned_at,files,artists,releases,recordings,tracks`. These are entity/file counts, so recordings and tracks need not be equal: one performance can have several local placements. The timestamp is Unix seconds. An unavailable catalog returns `503 catalog_unavailable`.

### HEAD /api/v1/library

Same loaded-catalog check, headers/status only.

## GET /api/v1/doctor

Parameters: optional `severity=error|warning|info`, `offset`, `limit`.

```sh
curl 'http://127.0.0.1:8787/api/v1/doctor?severity=warning&limit=20'
```

Returns a page plus `summary`, `unverified_files`, `pending_analyses`. `summary` counts all matching issues before pagination, grouped into error/warning/info. Each issue contains `type,label,severity,detail,files,file_count,files_truncated`. `type` is a machine-readable issue name; `label` and `detail` explain it. `files` previews at most 20 paths; `file_count` gives the full number and `files_truncated` tells you the preview is incomplete.

Doctor reads current catalog, conclusions and external claims under the data lock, including integrity results saved since the last scan. It reports findings, not repairs. `unverified_files` means no saved integrity verdict, not automatically a damaged file. `pending_analyses` concerns analysis work/results. `409 store_busy` means another writer holds the lock; retry after it finishes. Invalid severity gives 400; unreadable catalog/conclusions or sources give 500, never a misleading clean report.

### HEAD /api/v1/doctor

Runs the same validation/inspection without sending findings. HEAD does not trigger an audio integrity check either.

## GET /api/v1/stats

Parameters: only `offset`, `limit`. No `sort` or text filter.

```sh
curl 'http://127.0.0.1:8787/api/v1/stats?limit=10'
```

Top-level fields are `scanned_at,files,tracks,albums,compilations,artists,album_artists,labels,genres,duration_ms,bytes,tracks_without_album,completeness`. Completeness gives `covers,years,genres,mbid` as ratios, not percentages. `by_codec,by_quality,by_sample_rate,by_decade,by_country,roles` and `top.artists,top.writers` are separately paginated tables: the same offset/limit applies to **each**, not the top-level totals.

Ordinary bucket rows contain `label,count,bytes`; country buckets count artists and their `bytes:0` means not measured, not that artists' files take no space. Role rows contain `role,artists,credits`; top-artist/writer rows contain `reference,name,tracks`. Durations/bytes are local catalog totals. Source failure returns `sources_unavailable`; worker saturation returns `429 inspection_busy`.

### HEAD /api/v1/stats

Validates and calculates the same view but omits the tables/body.

## GET /api/v1/roots

Only `offset`, `limit` are accepted.

```sh
curl http://127.0.0.1:8787/api/v1/roots
```

Returns a page and whole-library `totals:{tracks,duration_ms,bytes}`. Rows contain `status,path,tracks,duration_ms,bytes`. `watched` is a scanned root, `excluded` a stored excluded root, and a synthetic `unwatched` row represents tracks outside the watched roots, with `path:null`. Overlapping roots can count the same track in different rows; whole-library totals count each track once. This route does not change roots or rescan folders. Unsupported parameters give 400.

### HEAD /api/v1/roots

Same inspection with no root list body.

## GET /api/v1/roles

Only `offset`, `limit` are accepted.

```sh
curl http://127.0.0.1:8787/api/v1/roles
```

Each item is `{role,artists,credits}` for a role actually present in this catalog. `artists` counts participating artists; `credits` counts credits using that role. The route does not list every theoretically possible role or filter artists by role. Unsupported filters return 400.

### HEAD /api/v1/roles

Same validation/view, no body.

## GET /api/v1/countries

Parameters: optional `q` (country/area name, ISO code or Aède-derived initials), `offset`, `limit`. Fixed artist-count/name ordering; no sort override.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/countries' --data-urlencode 'q=France'
```

The page adds `source,matched_by,coverage`. Items contain `name,iso_code,derived_initials,artists,tracks,duration_ms,bytes`. `iso_code` is an official code when available; `derived_initials` is a navigation aid, not an official country code. A place can be a region. Facts come from stored MusicBrainz data, not pressing-country tags or a new lookup. `matched_by` is `exact`, `partial` or null when unfiltered.

Coverage fields are `artists_total,artists_asked,artists_with_area,artists_without_area,places_without_iso_code`. They keep unknown origins visible. `q` with no known place match returns `400 invalid_query`; this differs from ordinary empty browse lists. Stored source errors give 500, not a claim that everyone has unknown origin.

### HEAD /api/v1/countries

Same matching, coverage calculation and errors, without JSON.

## GET /api/v1/years

Only `offset`, `limit`; chronological order with no override.

```sh
curl 'http://127.0.0.1:8787/api/v1/years?limit=20'
```

The page adds `albums_total,albums_without_year,tracks_without_dated_album`. Items contain `year,albums,tracks,duration_ms,bytes`. The year is the album's stored year. Unknown-year albums are counted separately rather than placed under year zero. The track coverage field also includes tracks lacking a dated album. Unsupported sorting/filtering gives 400.

### HEAD /api/v1/years

Same chronological view and status with no body.

## GET /api/v1/releases

This original version-1 list is preserved for existing clients. Prefer `/albums` when you want named artist/genre/label filters. Parameters are `q` (title), `artist` (artist **reference only**), `year`, `sort=catalog|title|year`, `order`, `offset`, `limit`.

```sh
curl 'http://127.0.0.1:8787/api/v1/releases?year=1980&sort=title'
```

Returns page items `reference,title,year,album_artist,track_count,cover_path`, as described in [Albums](catalog.md). The artist filter matches the album artist only. A name such as `artist=AC/DC` is invalid here: resolve an artist reference first. A malformed/wrong-kind reference gives 400; an absent valid artist gives 404.

### HEAD /api/v1/releases

Same compatibility-list filters/status, without JSON.

## GET /api/v1/entities

Exactly one required `ref` parameter; no name selector or pagination.

```sh
# Copy the reference from a previous response; replace this illustrative value.
curl --get 'http://127.0.0.1:8787/api/v1/entities' --data-urlencode 'ref=artist:REFERENCE_FROM_RESPONSE'
```

Returns one generic detail with `kind,reference` and kind-specific fields. Artist: `name,sort_name,mbid,aliases,releases`. Release: `title,year,album_artist,tracks,release_group,labels,cover_path`. Track: `title,release,recording,duration_ms,path,size,analyses`. Recording: `title,isrc,mbid,tracks,works`. Work: `title,mbid,recordings`. Release group: `title,mbid,releases`. Label: `name,mbid,releases`. Genre: `name,releases,tracks`.

Unlike `/artist`, this generic artist detail does not add `origin`. Related values are references; lists are complete. Missing/malformed reference returns `400 invalid_reference`, a valid absent entity 404. This route inspects one graph entity, not an arbitrary filesystem path.

### HEAD /api/v1/entities

Validates/resolves the same reference and omits its detail body.

The [API contract](../api.md) is the technical schema reference. Next: [Search and query](search.md).
