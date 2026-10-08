# Native desktop library and media work

Native clients use the existing account bearer authentication, JSON errors and bounded inspection workers. No origin exception or automatic enrichment is added.

## Local catalog projection

`GET /api/v1/presentation/artists` and `GET /api/v1/presentation/albums` return ordinary pages (`offset`, `limit` 1–200, `total`, `items`, `scanned_at`). Artist rows retain stable references, genre names, trusted origin with attribution, biography with its inseparable URL/language/licence, local image availability, release contributions and tag-derived collaborator links. Album rows retain compilations without inventing an album artist, contributing artists, local cover availability and a search projection of every raw tag field. The projection contains public catalog facts, never account annotations. Fields use snake_case like the existing API. Unknown origins remain null; no download is triggered by GET.

`GET /api/v1/artwork?ref=TOKEN&kind=cover|portrait|banner|background|spectrum&size=512` serves only images selected from the catalog or the existing artist artwork location. `size` is optional, bounded to 1–2048. Entity/kind combinations are validated. This endpoint reuses Subsonic's bounded, confined, opened-descriptor JPEG/PNG checks and response-only thumbnail generation. It never accepts a file path or external URL, writes a source image or redirects. Native transfers retain their admission permit until the response body is dropped. Accounts are rechecked after rendering; missing/changed/unsupported images fail explicitly. Track detail additionally reports `spectrum_available`.

## Own annotations and M3U

`GET /api/me/v1/annotations` returns paged current-account annotations for resolved catalog entities; other owners and unresolved targets are excluded. The existing singular GET/PUT endpoint remains the editing contract. Private data stays outside public catalog projection and cache.

`POST /api/me/v1/m3u` accepts `{"release":"TOKEN","name":"My album list"}`. A mutable account is required. It selects the catalog release's ordered tracks, creates a private static playlist using the shared personal store, and publishes a new M3U inside `data/playlists/OWNER/PLAYLIST-ID.m3u` without modifying music folders. Paths and names cannot choose the destination. The response contains `id`, `name`, `filename` and complete `content`. `GET /api/me/v1/m3u` pages private playlist summaries; `GET /api/me/v1/m3u/{id}` renders a complete download for the current owner. Rendering preserves repetitions and fails on missing tracks or newline-bearing paths. Paths refer to the server's music files; portable copy-path mapping remains the copy engine's responsibility. Publication holds the store lock; a failed private-store save removes only the newly created file.

## Explicit media jobs

`POST /api/admin/v1/analyze` and `POST /api/admin/v1/spectrum` accept `{"release":"TOKEN","force":false,"threads":2}`. Only the existing local administrative listener exposes these routes. `release` resolves to one catalog folder; arbitrary paths/arguments and unknown fields are refused. `threads` is optional, 0–64. `force` repeats cached analysis or redraws existing spectra. They return HTTP 202 with the existing task ID/status URL, and share task admission, polling, cancellation, bounded output and catalog refresh. An accepted task is not a completed task.

The CLI callback executes ordinary `analyze --json-layout=album --show-results` or `spectrum` with literal arguments. Analyses and spectra may create/update derived sidecars beside the server's music; audio and tags are unchanged. Test executions must use isolated copied music, not the owner's original library. Cancellation follows the existing administrative child-process policy.

Physical desktop playback, output selection and player copying remain separate client/device acceptance work; these read/projection/job endpoints do not implement them.

## FlacCompagnon report downloads

`GET /api/v1/analysis/report?ref=TOKEN` selects one current catalog track or release. `HEAD` validates the same selection without returning a body. The JSON response has the original `flaccompagnon-report` wrapper (`format`, `version`, `report.root`, `report.files`, `report.has_flac`) and an attachment filename. A track selects exactly its file entry; a release gathers distinct files in catalog track order, independent of whether original reports were stored per track, album or artist. All retained source-entry fields, including unknown future fields and original paths after relocation, are preserved. The root describes the selected catalog folder. Original sidecars are not reopened, and no scan, enrichment, re-analysis or filesystem write runs.

The existing account/read authorization and bounded inspection admission apply. Responses are `no-store` JSON with `nosniff`; the account is rechecked before returning. Only `ref` is accepted, for a track or release, never a path. Selection is bounded to 2048 tracks and 16 MiB. Missing entity/analysis returns 404; stale records, legacy records without complete source entries, or an unsupported schema return 409; oversized selections return 413. Albums with missing analyses are refused as incomplete rather than presented as a complete report. Import a complete newer report or explicitly run analysis to fill missing entries. Preserved error entries remain genuine source results; no measurement is invented. A file's validity uses the existing catalog size/mtime rule, not an extra live filesystem check.

## Dynamic browsing collections

A collection can retain a `browse-v1:` JSON expression with required `view` (`artists`, `albums`, `compilations`, `countries`, `genres`), `mode` (`none`, `years`, `alphabet`, `both`), inclusive `years`/`alphabet` pairs, `includeUnknownYear` and `query`, plus optional `countries` (union of sourced origin strings or null) and `genre`. Alphabet buckets are #=0 and A–Z=1–26. Hidden rails do not filter. Artist views match principal album artists and select their associated editions satisfying the other criteria; album views use album-title initials. Compilation view restricts editions explicitly labelled compilations. All selected editions contribute their complete track order.

The expression remains in the existing saved-query store; evaluation evolves with catalog, trusted origin/prose and current-owner annotations. Browse search folds case and common Western accents/ligatures while retaining punctuation and articles; it covers projected artist/album metadata, local tag names/values and own notes/tags/ratings/favourites. It does not invent countries from release-country tags or fetch enrichment. Unknown keys, invalid bounds/types and oversized expressions fail explicitly. Plain query collections keep their grammar and behavior. Public `/query` refuses browsing collections because they can read private annotations; the current-owner collection evaluator and CLI use the correct owner.

`GET /api/v1/presentation/playback?ref=…` exposes server-selected chronological principal discography or one album, including per-album track number and total (4096-item bound). It shares the CLI ordering function and does not start playback. See the route README for authenticated evaluation and confirmed history reset.

`GET /api/me/v1/collection/albums?name=…` evaluates the saved rule for the authenticated owner and returns current release references. `DELETE /api/me/v1/history` requires `{ "confirm": true }` to clear that owner’s events, scrobbles and counters together, preserving annotations and other owners.
