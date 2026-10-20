# MusicBrainz identity review

This is an explicit administrator operation on the common catalogue, equivalent to the CLI `review` command. It never retags or modifies source audio. It does not expose personal annotations or accept an owner argument. `source_review` in `/api/v1/status` advertises the routes when administration is configured; they use the existing HTTP/HTTPS administrative authentication and same-origin rules. Users, auditors and anonymous callers cannot read or change this review queue.

## GET /api/admin/v1/source-review

Lists stored MusicBrainz claims attached to the current catalogue that require or retain a human review. No external service is contacted. Query fields are `offset`, `limit` (the ordinary catalogue page bounds), `q` (normalized local-name substring, at most 1024 bytes), and `all` (boolean, false by default). Unknown parameters are errors. The default excludes accepted/rejected claims; `all=true` includes them. Orphaned claims and other providers remain stored but are not listed here.

The response contains `items,total,offset,limit`. Each item contains:

- `id`: the full review selector, not a prefix.
- `revision`: an opaque optimistic content stamp; it is not a credential.
- `reference,name,kind`: the current local catalogue entity.
- `source_id,url`: the proposed MusicBrainz identifier and an official HTTPS entity link, or null when no valid link can be constructed. A URL is never copied from arbitrary stored source content.
- `score`: the original search score, or null for identified claims.
- `decision`: null, `accepted` or `rejected`.
- `local_mbid`: the local identifier in the same MusicBrainz namespace as the claim, when available (artist, edition or release group, recording, label). It is also part of the optimistic revision, including for fuzzy proposals.
- `details`: bounded `[field,value]` evidence pairs such as type, region, country, dates and disambiguation. Local names and external facts remain distinct.

## PATCH /api/admin/v1/source-review

No query parameters. The bounded JSON body must contain exactly `id`, `revision` and `decision`, where the decision is `accept`, `reject` or `undo`. Full 16-hex-character selectors/stamps are required. The route takes the shared writer lock, rechecks the administrator session, reads current catalogue/source records from disk, and compares the revision before using the core review rules. It retains unrelated decisions, including decisions without a current source record.

A refreshed/replaced claim returns `409 review_changed`; reload the card instead of automatically replaying consent. A busy writer returns `409 store_busy`. Unreadable data refuses the operation without replacing the store. Successful writes return the updated item and publish a catalogue revision. `undo` removes that claim's explicit decision; it does not delete the external claim. A request already committed before client cancellation remains committed.

## Explicit biography retrieval

Review persistence and online retrieval are independent. `fetch_artist_summary_links` advertises an exact artist selector for `POST /api/admin/v1/fetch`: `{"summaries":true,"artist":"artist:example ensemble","full":true,"yes":true,"lang":"fr"}`. The `artist` value is a local stable reference, limited to 4096 bytes without NUL. It excludes positional targets and every other fetch pass. The CLI counterpart is `fetch --summaries --artist-ref=<reference>`; absent/ambiguous references fail rather than expanding by name.

For an already trusted artist whose abbreviated search claim lacks a Wikidata link, the summaries pass explicitly fetches detailed MusicBrainz relationships first. It preserves the reviewed source ID, original score, discography and human decision. Successful answers are saved progressively; a completed absence is retained with an optional `relationships_complete` marker, while malformed/failed answers remain retryable. Old records without that marker are incomplete, not confirmed empty. Provider rate limits and the existing background-job/cancellation rules still apply. MusicBrainz may genuinely have no Wikidata link or usable article. No Wikipedia name search is promoted into an identity automatically.

The detailed lookup must return a syntactically valid MusicBrainz UUID before its facts or completion marker can be published. A merged UUID may differ from the requested one; a malformed identifier is not evidence of a completed absence and leaves the stored claim unchanged and retryable.

Clients must capture the source and exact login across accept → fetch. If the login is replaced or the view disposed before fetch submission, they must not dispatch it. A failed fetch launch does not undo a successful acceptance. Once accepted by Aède, a job belongs to the existing task registry and survives client navigation; [task cancellation](jobs.md) preserves completed source answers.

A valid `artist:mbid:<UUID>` reference is resolved to the artist's catalogue key before summaries are scoped, just like the ordinary catalogue-key reference. A completed Wikipedia cache suppresses a new request only for the same Wikidata entity. If a newly trusted MusicBrainz identity provides a different Wikidata link, the older article stays stored but is not presented as that identity's biography; a pending proposal cannot invalidate independent evidence, and explicit reviews of the Wikipedia claim still take priority. An interrupted or failed retrieval retains completed link updates and the old article without displaying a known identity mismatch.

The same identity rule applies to Discogs label biographies: a recent profile for a different validated Discogs ID cannot suppress retrieval of the newly linked profile or establish trust in the old one. Cache age alone does not prove identity. A rejected profile for the same ID is not automatically downloaded again merely because it was rejected.
