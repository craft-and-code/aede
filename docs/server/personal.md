# Personal annotations, history and collections

These administrative routes require [administrative authentication](administration.md) and address `local`. Only the trusted installation token or an administrator session owning `local` can use them; other administrator sessions receive `403 forbidden`. [Account sessions](accounts.md) use the same operations at `/api/me/v1` for their own authenticated owner. Requests cannot select another owner. An `auditor` session may use the `GET` and `HEAD` forms only; every `PUT`, `POST` or `DELETE` personal operation returns `403 forbidden`. Operations read the current disk catalog and `user.json` under the shared lock, so selection and atomic updates use one completed catalog version.

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

Returns HTTP 200 with a page; items contain `track,at,ms_played,completed,play_count,last_played`. Times `at`/`last_played` are Unix seconds; played duration is milliseconds. Counts describe the owner's all-time track total, not just this page. Each owner retains their newest 500 native events; another owner's listening never removes them. Events are ordered by date; equal-date events remain distinct, with the last received first. Per-track counts survive older events dropping from that owner's recent log.

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

Required `name` (1–256 bytes); JSON `{expression:"…"}` with 1–8192 bytes of nonempty expression. The grammar is validated before saving. Existing normalized names are replaced; new names are created. A smart collection evolves as library/personal values change, unlike a static playlist.

```sh
curl -X PUT 'http://127.0.0.1:8787/api/admin/v1/collection?name=Favourites' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN" \
  -H 'Content-Type: application/json' -d '{"expression":"loved"}'
```

Returns HTTP 200 `{name,expression,created_at,updated_at}`. Invalid expression/size gives `400 invalid_parameters`, invalid name `invalid_query`, wrong/unknown body fields `invalid_body`. Personal clauses are allowed in this owner's stored expression; public `/query` still refuses them. Saving does not export M3U or create a playback task.

## PATCH /api/me/v1/collection

Required existing `name` query; JSON `{ "name": "New name" }` changes only that collection’s display name, retaining its expression and creation time. The transitional `/api/admin/v1/collection` route supports the same method for the local owner. Names retain the existing 1–256-byte, case/accent-insensitive matching policy. `collection_rename` advertises this capability. Another collection under the requested name gives `409 collection_exists`; a missing original gives `404 collection_not_found`. Invalid names/body fields are refused. Auditors cannot rename, other owners remain inaccessible and revocation is rechecked under the single writer lock. One atomic personal-store save publishes the new name; no create/delete sequence can leave duplicates or lose the rule on a failed second write.

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

The native `Play` journal and the separate Subsonic `Scrobble` journal each retain up to 500 events per owner. Their independent global capacity is 128,000 events per journal (500 × the account capacity of 256). `user.json` input and its indented serialized representation are limited to 64 MiB, including escaping and whitespace. Load/import and save refuse an exceeded bound; they do not trim another owner's records or replace the existing file. HTTP load failures use `user_unavailable`, and publication failures use `store_error`. These are storage budgets, not a bound on total process memory: the parsed store, domain values and JSON representation also occupy memory. No format change or automatic migration is performed; counts are retained and previously discarded events cannot be reconstructed.

Static playlists have no native `/api/me/v1` route yet, but the [Subsonic/OpenSubsonic adapter](subsonic.md) reads and writes private ordered playlists in the same personal store. That adapter also serves catalogued JPEG/PNG sidecar artwork and original audio, and records client-declared scrobbles separately from `Play` events: no played duration or completion is invented. Both kinds contribute to all-time track counts; the native history page lists `Play` events only. An ephemeral, owner-private now-playing declaration does not increment counts or create history.

Relation annotations, source review, check/analyze/fingerprint, copy, backup/restore, reset, merge, arbitrary files and shell commands have no current HTTP routes. Lyrics/prose remain outside the HTTP contracts. Their CLI availability does not imply API availability.

## Repeated private M3U saves

The M3U summary endpoint groups historical saves only when owner, exact name and ordered track references all agree. It exposes the newest matching playlist ID and a `duplicates` count; different names, orders, repeated track occurrences or owners remain distinct. All existing records and export files remain stored. Repeated publication already reuses an identical selection and recreates a missing export; the list view does not silently delete history. The `grouped_m3u_exports` capability advertises these summary semantics.

`GET /api/me/v1/m3u` accepts `offset`, `limit` and optional `q`. Search normalizes names using the existing case/accent-insensitive catalog matching and filters all of the authenticated owner's grouped summaries before pagination. `q` must contain searchable text and fit within 2048 UTF-8 bytes. Support for this additional search parameter is advertised by `library_m3u_files`; clients talking to older servers must omit it. Private export IDs, publication and downloads keep their existing contracts.

## M3U files already present in watched roots

The `library_m3u_files` capability advertises two additive, authenticated, read-only routes. They expose shared library files, separately from retained private exports; no playlist is imported into `user.json` or rewritten. An account session, including an auditor, may read them. An administrative token alone does not authorize `/api/me/v1`.

The private publication subtree `data_dir/playlists` is always implicitly excluded from both file listing and detail, independently of configured exclusions, even when a watched root encompasses the data folder. A path-derived file identity cannot bypass this boundary. Existing `/api/me/v1/m3u` summaries and downloads continue to expose only the authenticated owner's private exports.

| Route | Parameters | Response |
| --- | --- | --- |
| `GET /api/me/v1/m3u-files` | `offset`, `limit`, optional `q`; no body | Standard page `{items,total,offset,limit,scanned_at}` plus `unavailable_roots` and `unreadable_directories`. Items are `{id,name,filename,path,track_count:null}`. |
| `GET /api/me/v1/m3u-files/{id}` | No query or body | `{id,name,filename,path,track_count,content}`. |

Pagination defaults to 50 items and permits 1–200. Search has the same text validation as private exports, matches the full canonical path (including the filename), and occurs before pagination. Files sort by canonical path, independent of filesystem enumeration order. Discovery includes every regular `.m3u` or `.m3u8` file, case-insensitively, including folders without catalogued audio. Canonical aliases and overlapping watched roots are deduplicated. Excluded subtrees and child symlink files/directories are omitted; a watched root may itself be a canonicalizable directory alias. Non-UTF-8 playlist paths or paths over 4096 UTF-8 bytes produce an explicit error rather than a lossy or incomplete list.

Listing does not open playlist content. Invalid or oversized files are therefore visible and may produce an error when selected. The detail preserves exact UTF-8 text, including BOM, CRLF, whitespace and filename extension. Both shared CLI renderings, extended M3U and `playlist --simple`, are admitted. `track_count` counts nonempty, noncomment entry lines, including repeated entries and URL lines; it does not assert that those entries are playable. Playlist entry paths, directives and URLs never cause filesystem resolution, audio reads or network requests. Binary/control-character content other than tab/CR/LF is refused. Legacy files in other encodings are listed but require an explicit user conversion outside this API before their content can be read.

Traversal is bounded to 10,000 playlist files, 250,000 filesystem entries, depth 64 and a cooperative three-second inspection deadline. A limit refuses the response instead of publishing a partial successful total. Missing roots and inaccessible directory operations/entries increment the two diagnostic counts, so a successful page may explicitly describe an incomplete accessible subset. Detail input is limited to 8 MiB and 4096 entry lines; serialized JSON is limited to 16 MiB, including escaping. Blocking inspections use the existing bounded worker slots and retain their slot after client disconnection. The deadline cannot interrupt an already blocked filesystem syscall, including a network filesystem operation.

The `m3u-file-` identity followed by 64 lowercase hexadecimal characters is a deterministic path selector, not an authorization token. A detail request rediscovers its identity under the current watched roots and exclusions, refusing ambiguous identities. Rename/move may change the identity. Session revocation and catalog roots/exclusions/scan version are rechecked before publication; root aliases and selected path/file metadata are also checked around reads. These checks detect ordinary replacement and scope changes but are not descriptor-relative filesystem confinement: a hostile concurrent rename/symlink exchange remains subject to TOCTOU limits. Unix descriptor checks include device/inode; on other platforms size/type/modification checks provide weaker replacement detection. No stronger Windows race guarantee is implied.

Malformed identities or unsupported parameters give `400 invalid_query`; a file absent from current permitted roots gives `404 m3u_file_not_found`; detected replacements/scope changes give `409 m3u_files_changed`; traversal/content/entry/response limits give `413 m3u_files_too_large`; unsupported paths or text give `422 m3u_path_invalid` / `m3u_text_invalid`; unavailable reads and elapsed inspection deadlines give `503 m3u_file_unavailable` / `m3u_files_timeout`. Existing personal-worker saturation and session errors also apply. Refresh or retry remains explicit; errors never rewrite source files or private stores.

## Account presentation preferences and atmospheres

`GET /api/me/v1/preferences` returns `{values:{key:JSON-value}}` for the authenticated owner. `PATCH` accepts `{values:{...}}`, merging only supplied keys rather than replacing other settings. Keys use the `phemios.` namespace (128 ASCII bytes, alphanumeric plus `._:-`), values are valid JSON up to 8192 bytes, and each owner retains at most 64 entries. A patch contains 1–32 entries. Auditors can read only. These operations require no scanned catalog, recheck the principal under the store lock, preserve unrelated annotations/owners and refuse corrupt stores. Optional `client_preferences` in `user.json` remains compatible with older stores. The `account_preferences` capability advertises this contract; credentials and installation roots do not belong in it.

`GET /api/me/v1/moods` returns `{moods:[{rule,count}],items,total,offset,scanned_at}`. Optional `mood` selects one rule; `offset`/`limit` use ordinary personal pagination (maximum 200). `PUT` accepts `{id,name,tags,genres}` to create or override one owner-private rule. Up to 32 custom rules are retained, each with at most 32 exact normalized tags and 32 genres; the combined stored JSON is bounded to 8192 bytes. Unknown rule IDs on reads, empty criteria and malformed rules are refused. Starter rules are transparent suggestions, not measured emotions.

The `aede-moods` crate matches explicit tag/genre evidence. A track inherits its own annotations, its album's annotations and the principal album artist's annotations; guest credits and other owners are excluded. Catalog genre links use those same targets. A recognized personal tag on the track overrides recognized tags inherited from the album and principal artist; the album overrides the artist. The vocabulary includes the owner's custom and overridden rules. Unrelated tags do not suppress fallback. When a recognized personal tag exists, catalog genres do not select contradictory atmospheres; otherwise exact genres remain the fallback. Each result includes `{reason:{tag,genre,scope}}`, where `scope` is `track`, `album`, `artist` or `genre`, and the existing playback-selection fields; only visible rows are expanded. Results prioritize that evidence scope before pagination, retaining chronological release/track order within equal scopes. Scope is an evidence level, not a confidence or emotion score. Rules are stored as owner-private `phemios.moods` preferences; ordinary reads do not contact providers, decode audio or run FlacCompagnon. `mood_rules` advertises the routes.

## Owner-scoped advanced search and queue publication

`GET /api/me/v1/search` accepts `expression` (at most 8192 bytes), `offset` and `limit` (1–200). It evaluates the same grammar as saved collections against the current authenticated owner's annotations, under the catalog/user lock, and returns the standard lightweight track page. Auditors can read it; no other owner selector is admitted. Status advertises `advanced_search`. Technical clauses add `bitdepth` and `channels`; absent declared values remain unknown. `ref:=track:…` identifies a stable track and can retain a selected queue's membership in a collection. Query collection results remain dynamic and do not retain playlist order or repeated occurrences.

`POST /api/me/v1/m3u` additionally accepts `{name,tracks:["track:…",…]}` instead of `release`. Exactly one selector is required. The nonempty ordered list is bounded at 4096 occurrences and by the existing 16 KiB body limit. Repeated tracks remain repeated; unknown tracks and mixed selectors are refused. Folder-sidecar publication still requires a release and administrator authorization. Status advertises `queue_m3u`.

## Opaque achievements

`GET /api/me/v1/achievements` projects the current account's progress. `POST` with `{}` retains newly unlocked entries; `{ "advanced_search": true }` declares a successful client search action. Auditors may only read. `private_achievements` advertises support. Locked entries contain only an anonymous slot and null date; definitions remain engine-only. Unlock dates are monotonic and use retained private evidence, including confirmed completed visits. Reserved engine state is excluded from `/api/me/v1/preferences`. Unreadable records are retained; optional projection failure cannot reject a valid annotation or history write. Local and remote accounts have independent progress.


Collection list windows also include `duration_ms` when every matching track has a known duration. The value evaluates the saved rule for the authenticated owner against the same locked catalog; only the requested page is measured. Unknown durations, invalid legacy rules and arithmetic overflow omit the value; a known empty selection is zero. No source files or annotations are changed.


### Opt-in audio features for atmospheres

Capability `mood_audio_features` accepts `audio=true` on mood reads. It adds shared descriptor coverage, a selection revision token, four immutable measured recipes and current-owner exemplar proximity; ordinary reads retain the legacy response and selection. Optional `revision` must be a sixteen-character lowercase hex token and refuses changed acoustic/source/private criteria with HTTP 409. Reads may warm a bounded on-disk descriptor cache but never decode audio. Measured criteria and explanations do not infer emotional truth. See [cooperative acoustic discovery](../design/audio-features.md).

Trusted-loopback administrators with `mood_audio_extraction` may explicitly POST `/api/admin/v1/mood-features` with `{force:false}`. The native `mood_features` task reports `{phase,done,total,analyzed,reused,failed}` in the existing authenticated task read/list response. One extraction is admitted at a time; Stop uses the ordinary empty-body cancellation route. Completed records survive interruption. HTTPS exposes this administrative route only to a named administrator session. The first profile has an explicit 16,384-file catalog budget.

Authenticated POST `/api/me/v1/background-priority` accepts `{client:<UUID>,active:<boolean>}` to renew/remove an owner-scoped six-second foreground lease, protecting WebKit/external audio while background extraction is active. Eight leases per owner and sixty-four overall are admitted; expired leases are retired. One owner cannot clear another owner's lease. No private classification or personal-store mutation is transmitted.

## Owner-private album listening

`GET /api/me/v1/album-listening?offset=0&limit=200`, advertised by `album_listening`, returns `{total,offset,items:[{reference,completed}]}` for the current catalog editions and the authenticated owner. Standard bounded pagination applies. It accepts no body, owner selector or mutation method; auditors may read their own state. Captured sessions are rechecked before publication. Public catalog responses and caches never contain these personal flags.

An edition is completed when every nonempty catalogued track placement has a completed `Play` event for that owner. Different sessions may contribute. Partial visits, seeking interrupted visits and duration-less scrobbles do not establish completion. Native acknowledgement rules are unchanged; the existing explicit personal-history POST remains a client declaration, not physical-output verification. All-time completed counts survive the owner's 500-event recent-history retention and follow existing backup, merge and explicit reference-relink rules. Resetting that owner's history clears its evidence; already retained trophies remain unlocked.

The optional `completed_count` in `user.json` counters is backward compatible. Missing legacy values recover only retained completed events, bounded by the all-time count; earlier evicted visits cannot be invented. Malformed or contradictory explicit counts refuse the store without replacing it. CLI `albums` and `album` display `◷` beside editions without full-listening evidence for the same legacy local owner used by CLI annotations; machine-readable selections/exports remain unchanged.
