# Albums, artists, tracks and recordings

These routes read the current local catalog. They do not fetch information online, change tags or start playback. See [HTTP basics](http.md) for pagination, URL encoding, name ambiguity and errors. All GET requests below return HTTP 200 on success; their HEAD counterparts return identical status/headers without a body.

## GET /api/v1/albums

Browse album summaries, one row per local release/edition. Optional parameters are `q` **or** `name`, `artist`, `year`, `genre`, `label`, `mbid`, `sort`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/albums' \
  --data-urlencode 'artist=AC/DC' --data-urlencode 'year=1980' \
  --data-urlencode 'sort=title' --data-urlencode 'limit=10'
```

`q`/`name` searches the album title by normalized substring. Artist, genre and label filters accept a partial name or a reference of the corresponding kind. Artist means **album artist**, not every musician credited. Genre considers both album and track links. Filters combine with AND; an unknown referenced/named filter is an error. `year` is an exact unsigned 32-bit integer; `mbid` is an exact case-sensitive identifier. Title sorts accept `sort=name|title`; `year` and `catalog` are also supported, with `order=asc|desc`.

The page's `items` contain `reference,title,year,album_artist,track_count,cover_path`. Follow `album_artist` through the artist route. `cover_path` is a local path, not an image-download URL. `null` means unavailable. Two editions can have the same title; use their references for detail. Invalid/duplicate parameters return `400 invalid_query` (or pagination errors); no matching albums normally means an empty page.

### HEAD /api/v1/albums

Uses the same filters/validation. `curl -I 'http://127.0.0.1:8787/api/v1/albums?limit=10'` checks the response without downloading JSON.

## GET /api/v1/album

Select one album with exactly one `name` or `ref`; no pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/album' --data-urlencode 'name=Back in Black'
```

The object has `kind:"release",reference,title,year,album_artist,tracks,release_group,labels,cover_path`. `tracks` contains the complete list of track references, not audio or nested track objects. Follow those with `/track`. `release_group` connects editions of the same album; `labels` links its labels. Unknown values are null and absent links are empty arrays. A duplicate title produces `409 ambiguous_entity`; choose a returned reference. A missing album returns `404 entity_not_found`.

### HEAD /api/v1/album

Accepts the same single selector; returns the selection's status/headers with no detail body. Missing or ambiguous albums remain 404/409.

## GET /api/v1/artists

Browse artist summaries with `q`, exact `mbid`, `sort=catalog|name`, `order`, `offset`, `limit`. Search examines names and stored aliases; `name` sort uses the artist's `sort_name`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/artists' \
  --data-urlencode 'q=Miles' --data-urlencode 'sort=name'
```

Each item is `{reference,name,sort_name,mbid,aliases}`. Aliases are an array, possibly empty. `mbid:null` means no local identifier. No result is an empty page; unsupported parameters or malformed exact filters are refused. The CLI's `--role` and `--country` are not parameters on this route; use `/roles` or `/countries` to inspect those dimensions.

### HEAD /api/v1/artists

Validates the same list request but omits its page body, for example `curl -I 'http://127.0.0.1:8787/api/v1/artists?limit=1'`.

## GET /api/v1/artist

Select one artist by exactly one `ref` or `name`; aliases are accepted. No pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/artist' --data-urlencode 'name=Miles Davis'
```

The detail has `kind:"artist",reference,name,sort_name,mbid,aliases,releases,origin`. `releases` is a complete array of album references. `origin` is an attributed stored fact, not a guess from an album's pressing country. Its fields are described under `/from` below. This detail does not reproduce every CLI panel, biography or relation view. Ambiguous names return 409 with candidates; missing artists return 404.

### HEAD /api/v1/artist

Checks the same selection and gives headers/status only; no biography or JSON is sent.

## GET /api/v1/from

Read only one artist's origin, selected by exactly one `ref` or `name` without pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/from' --data-urlencode 'name=Miles Davis'
```

The response is `{reference,name,origin}`. An unknown origin is explicitly represented:

```json
{"status":"unknown","country_code":null,"area":null,"message":"No MusicBrainz information has been fetched for this artist; GET never fetches it automatically.","attribution":null}
```

This example is the `origin` object, not the full response. `status:"known"` means at least a country code or area is available; either can still be null. An area can be a region. Attribution contains `source,source_id,fetched_at,confidence,match_score,trusted`; confidence is `identified` or `matched`, and an identified lookup has null `match_score`. Only trusted, nonconflicting stored MusicBrainz identities establish origin. GET never fetches a missing fact. Human `message` text is not a stable code. Selection errors are the same as `/artist`; unreadable source stores return `500 sources_unavailable` rather than a false unknown answer.

### HEAD /api/v1/from

Performs the same selection/source validation with no origin body.

## GET /api/v1/tracks

Browse track placements with `q` (title substring), `release` (an album/release **reference**), `sort=catalog|title`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/tracks' \
  --data-urlencode 'q=Hells Bells' --data-urlencode 'limit=10'
```

Items contain `reference,title,release,recording,duration_ms`. `duration_ms` uses milliseconds; null means unknown. An album title is not valid for `release`: obtain its reference with `/album` first. Wrong-kind/malformed references return 400; a valid reference absent from this catalog returns 404. Empty title matches return an empty page. A track can lack an album or recording link.

### HEAD /api/v1/tracks

Same filters/status, without JSON. `curl -I 'http://127.0.0.1:8787/api/v1/tracks?limit=1'` checks this route.

## GET /api/v1/track

Select a track by exactly one `ref` or `name`, without pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/track' --data-urlencode 'name=Hells Bells'
```

The detail contains `kind:"track",reference,title,release,recording,duration_ms,path,size,analyses`. `path` is the local music-file path and `size` is bytes. `analyses` contains attributed acoustic-analysis entries: `source,source_version,imported_at,stale`, measurement fields and `source_data`. Complete original analysis data is retained in `source_data` when available; older imports may have null there. `stale` identifies results that no longer match the current file facts. These are separate source results, not rewritten local tags. The endpoint does not analyze the file or stream its audio. Duplicate track titles often occur across editions; select a reference after a 409. Unknown tracks give 404.

### HEAD /api/v1/track

Checks the same selector and returns no file facts/analysis body. HEAD does not read or transmit audio.

## GET /api/v1/recordings

Browse recorded performances with `q` (title), `work` (a work reference), `sort=catalog|title`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/recordings' --data-urlencode 'q=Hells Bells'
```

Items contain `reference,title,mbid,track_count,work_count`. One performance can appear in several local album editions, hence several tracks. `work_count` counts linked compositions, not files. The work filter follows explicit catalog links; malformed/wrong-kind references return 400 and absent valid references 404. No matches gives an empty page.

### HEAD /api/v1/recordings

Same list validation, headers/status only; for example `curl -I 'http://127.0.0.1:8787/api/v1/recordings?limit=1'`.

## GET /api/v1/recording

Select one performance by exactly one `ref` or `name`; a MusicBrainz identifier also works through `name`. No pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/recording' --data-urlencode 'name=Hells Bells'
```

The object has `kind:"recording",reference,title,isrc,mbid,tracks,works`. `isrc` identifies a recording when present, while `mbid` is its MusicBrainz identity. `tracks` and `works` are complete reference arrays; use `/track` and `/work` to follow them. A fetched claim without a corresponding local graph entity is not automatically promoted into a recording. Ambiguous selections return 409; absent recordings return 404.

### HEAD /api/v1/recording

Selects/validates the same entity and returns status/headers without detail JSON.

Next: [Genres, labels, works and editions](graph.md), [Search](search.md), [Inspection](inspection.md).
