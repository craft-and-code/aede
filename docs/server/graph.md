# Genres, labels, works and editions

These graph-navigation routes use the [common page and selector rules](http.md). GET returns HTTP 200 on success; HEAD validates the same parameters and returns the same status/headers with no body. Detail relationships are complete arrays, not paginated lists. Data comes from the local catalog, never an automatic web lookup.

## GET /api/v1/genres

Parameters: `q` or `name` (normalized name substring), `sort=catalog|name`, `order=asc|desc`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/genres' --data-urlencode 'q=jazz'
```

Each page item contains `kind:"genre",reference,name,release_count,track_count`. The counts describe graph links, not whether every file has an explicit genre tag. No match is an empty page. Passing both text selectors, duplicate parameters or an unsupported sort gives 400.

### HEAD /api/v1/genres

Same filters, without page JSON; `curl -I 'http://127.0.0.1:8787/api/v1/genres?limit=1'`.

## GET /api/v1/genre

Exactly one `ref` or `name`; no pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/genre' --data-urlencode 'name=Jazz'
```

Returns `kind:"genre",reference,name,releases,tracks`. Track links include inherited album genres, so a track can appear without its own genre tag. Follow references with `/album` and `/track`. Ambiguous names return 409/candidates; absent genres return 404.

### HEAD /api/v1/genre

Same selection/status, with no detail body.

## GET /api/v1/labels

Parameters: `q` or `name`, exact case-sensitive `mbid`, `sort=catalog|name`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/labels' --data-urlencode 'q=Blue Note'
```

Each item is `kind:"label",reference,name,mbid,release_count`; `mbid` can be null. This is the catalog's labels, not a label biography or an automatic Discogs request. No matches gives an empty page; unsupported/duplicate parameters give 400.

### HEAD /api/v1/labels

Same list validation with no body; `curl -I 'http://127.0.0.1:8787/api/v1/labels?limit=1'`.

## GET /api/v1/label

Exactly one `ref` or `name`; no pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/label' --data-urlencode 'name=Blue Note'
```

Returns `kind:"label",reference,name,mbid,releases`. Follow each album reference for its local edition. Missing labels return 404; ambiguous labels return 409 with candidates. This endpoint does not return every prose panel shown by the CLI.

### HEAD /api/v1/label

Validates the same label selection and omits its JSON body.

## GET /api/v1/works

A work is a composition, distinct from a recorded interpretation. Parameters: `q` or `name` (title substring), exact `mbid`, `sort=catalog|name|title`, `order`, `offset`, `limit`. `name`/`title` sorts are aliases.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/works' --data-urlencode 'q=Concerto'
```

Items contain `kind:"work",reference,title,mbid,recording_count`. Only works represented in the local graph appear. A fetched work claim alone does not create a local work. No result means an empty page, not proof that the composer never wrote it. Invalid parameters return 400.

### HEAD /api/v1/works

Same parameters/status without JSON; `curl -I 'http://127.0.0.1:8787/api/v1/works?limit=1'`.

## GET /api/v1/work

Exactly one `ref` or `name`; a MusicBrainz work identifier may be passed through `name`. No pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/work' --data-urlencode 'name=Concerto'
```

Returns `kind:"work",reference,title,mbid,recordings`. The array links local interpretations of that work; follow `/recording` and then `/track` to reach files. 409 means several compositions matched; 404 means none is represented locally. No missing work is fetched by this request.

### HEAD /api/v1/work

Same selection and errors, no detail body.

## GET /api/v1/release-groups

A release group describes an album identity shared by its editions, while a release is a particular local edition. Parameters: `q` or `name`, exact `mbid`, `sort=catalog|name|title`, `order`, `offset`, `limit`.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/release-groups' --data-urlencode 'q=Back in Black'
```

Items contain `kind:"release_group",reference,title,mbid,release_count`. `release_count` counts the local editions linked to this identity, not all editions worldwide. No result is an empty page; conflicting/unsupported parameters give 400.

### HEAD /api/v1/release-groups

Same validation and headers/status only; `curl -I 'http://127.0.0.1:8787/api/v1/release-groups?limit=1'`.

## GET /api/v1/release-group

Exactly one `ref` or `name`; MusicBrainz identifiers can be supplied as `name`. No pagination.

```sh
curl --get 'http://127.0.0.1:8787/api/v1/release-group' --data-urlencode 'name=Back in Black'
```

Returns `kind:"release_group",reference,title,mbid,releases`. Follow each reference through `/album` to compare local editions. Ambiguity returns 409/candidates, absence 404. This is neither the external full discography nor a missing-album calculation.

### HEAD /api/v1/release-group

Same entity resolution and status without JSON.

Next: [Inspection and compatibility routes](inspection.md).
