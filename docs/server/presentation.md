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
