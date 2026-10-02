# Search and query through HTTP

Search finds names across the graph. Query selects **tracks** with an expression. Both are public read routes; they never fetch missing data. Parameters are URL-encoded and list results use [normal pagination](http.md). Text is limited to 2048 bytes; public queries have at most 64 complexity units (terms, parentheses and negations).

## GET /api/v1/search

Required `q`; optional `comments=true|false` (false by default), `offset`, `limit`. No sort parameter.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/search' \
  --data-urlencode 'q=Miles Davis' --data-urlencode 'comments=false'
```

Returns HTTP 200 with a ranked page. Each item contains `kind,reference,name,context,found_in`. `kind` identifies the entity type; `context` provides a human-readable explanation. `found_in:"name"` is a name match. With `comments=true`, matching **stored file-tag comments** are appended with `found_in:"comment"`; these are not your personal Markdown notes. A track can appear twice if both its name and comment match. A nullable reference should be handled rather than treated as an array index.

An empty/oversized `q`, an unsupported parameter, duplicate parameters or comments other than literal true/false returns `400 invalid_query`; no matches yields an empty page. `429 inspection_busy` means retry after other inspections finish.

### HEAD /api/v1/search

Uses the same required query and performs the same validation/search, returning status/headers only. Example: `curl -I 'http://127.0.0.1:8787/api/v1/search?q=Miles'`.

## GET /api/v1/query

Required `q` (an Aède expression); optional `sort`, `offset`, `limit`. No separate `order` parameter: a trailing `-` on the sort reverses order.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/query' \
  --data-urlencode 'q=codec:flac year:>=1980' --data-urlencode 'sort=year-'
```

Returns HTTP 200 with track summaries `reference,title,release,recording,duration_ms`. Spaces mean AND; `|` means OR; prefix `-` negates a condition; parentheses group conditions. Quote values containing spaces inside the expression, for example `artist:"Miles Davis"`. The [query guide](../querying.md) explains supported public field values and relation predicates.

Public fields include title, artist, album, recording, work/movement, release group, album artist, genre, label, file comment/path/codec/year/duration/size/bitrate/sample rate, lossless/compilation and catalog credit/relationship predicates. Personal notes, ratings, favourites, user tags and play history are **refused**, as are lyrics. A public query cannot use these just because the same expression works in your CLI. Authenticated smart collections may store personal expressions, but that does not add a public collection-evaluation route.

Allowed sorts are `catalog,title,artist,album,year,duration,length,size`, each optionally suffixed with `-`; `length` aliases duration. Rating/played sorts are refused. Sorting precedes pagination. A syntactically valid expression naming an unknown catalog value can still be refused with `400 invalid_query`; Aède does not silently ignore such a term. Broken syntax, excessive complexity, private/lyrics clauses and unsupported options also give 400. Unreadable sources give 500; worker saturation gives 429.

### HEAD /api/v1/query

Validates/evaluates the same expression without returning its track page. Example: `curl -I 'http://127.0.0.1:8787/api/v1/query?q=codec%3Aflac'`.

Use [WebSocket events](events.md) to know when to refresh results after a catalog change.
