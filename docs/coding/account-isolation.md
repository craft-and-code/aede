# Account isolation audit

The catalog is shared between accounts. Watched roots, music metadata, external source claims, verified audio, artwork, diagnostics and catalog exports describe the installation's library. Personal statements and client state belong to an immutable account owner. This audit retains that boundary; it does not introduce per-account catalogs or private watched roots.

## Authorization and ownership

The native middleware authenticates a bearer session and obtains its stable owner from the protected account store. Request selectors and bodies cannot select an owner. Renaming a login does not transfer ownership. Account epoch/revision and the live session registry are checked again in blocking workers. The first administrator inherits the legacy `local` owner; subsequent administrators have distinct owners.

An administrator role permits installation jobs and account management, not another account's personal data. Transitional `/api/admin/v1/{annotation,history,collection,collections}` routes still address `local`, but now refuse any administrator session whose owner differs from `local`. `/api/me/v1` always addresses the session's own owner. The installation administrative token and the local CLI remain trusted OS-operator interfaces; they are not account isolation boundaries.

| Surface | Personal reads | Mutation boundary |
| --- | --- | --- |
| Native annotation and annotation index | `owner + EntityRef`; favourites, ratings, notes and tags never enter shared catalog responses | Same owner lookup/update; unknown owner fields refused; clearing one owner's statement preserves other owners |
| Native history and counters | Filtered by stable owner before pagination; counters match that owner and track | Record/reset only that owner; playback persists the captured authenticated owner and confirmed consumed audio |
| Collections and advanced search | Names are looked up within the owner namespace; query indexes/context use that owner for opinions and counts | Create, rename, replace and delete match owner plus normalized name; another owner's identically named collection is independent |
| Application preferences | Only that owner's `phemios.*` keys; reserved engine keys excluded | PATCH accepts only `phemios.*`, keeps other owners/keys, refuses capacity growth; no request owner selector |
| Achievements | Projection and persisted engine key use the same owner | Own actions only; client preference PATCH cannot replace `aede.achievements.v1` |
| Moods and missing-album decisions | Rules, opinion evidence and set-aside rows are owner-filtered | Own decisions/rules only; cached missing-album artwork is shared source artwork rather than a personal statement |
| Saved M3U exports and static playlists | ID lookup is owner-scoped; an opaque ID is a selector rather than authorization | New selection belongs to the authenticated owner; no sharing flag; publication in music folders requires installation administration |
| Library M3U files | Shared watched-root files, with implicit exclusion of `data_dir/playlists` | Read-only; private exports retain their separate owner-scoped API |
| Named native-player profiles | Lookup and active lease use `(owner, profile)` | Captured owner must match snapshot owner; revision/session conflict refuses replacement; queue/checkpoint persistence merges the shared store without replacing another owner's state |
| Subsonic/OpenSubsonic | A revocable client key identifies one stable owner; favourites, ratings, counts, playlists and temporary now-playing reports use that owner | Auditors cannot mutate; another owner's playlist ID/username grants no access; current key/role rechecked before store publication |
| Shared native catalog/navigation/inspection | No personal-store projection: public query contexts use empty personal data and reject private predicates where applicable | Installation metadata operations keep their existing administrative contract |

There are no dedicated backend podcast or radio stores/routes. Aède can retain bounded application JSON under owner-scoped `phemios.*` preferences; it does not interpret a feed/station schema or independently enforce client playback admission. Phémios's browser/native persistence and asynchronous session admission require their own audit. This report does not claim to isolate client-local storage.

## Concrete defects corrected

### One account could evict another account's recent history

Native `record_play`, declared `record_scrobble` and import/merge used a single 500-event journal limit. Alice's retained listen disappeared after Bob added enough later listens, despite each row carrying an owner. Per-track all-time counters were already owner-scoped, so the symptom affected the recent journal rather than their lifetime totals.

Retention now keeps the newest 500 native `Play` events **per owner** and the newest 500 `Scrobble` declarations **per owner**, independently. Event-time sorting and equal-time multiplicity are preserved. Loading and merging use the same policy; no scan or import restores a global eviction rule. Existing discarded events cannot be recovered by this change.

Each journal also has an independent global capacity of 128,000 rows, derived from 500 × the 256-account capacity. Reaching this capacity refuses publication instead of evicting another owner's data. Disabled/orphaned owners remain retained and still consume the store budget. `user.json` input and its indented serialized representation are capped at 64 MiB. The shared JSON renderer counts UTF-8, escaped strings/keys, numbers and formatting exactly; oversized output is refused before an atomic write, and embedded backup user documents receive the same validation without allocating a second serialized document. These are store/output budgets, not a total process-memory guarantee. Domain objects, the parsed JSON tree and serialization structures also occupy memory.

An oversized store is refused intact, not reset, migrated or partially trimmed. Native HTTP load/save failures retain their existing generic `user_unavailable`/`store_error` responses. A backup's unreadable user part is reported separately from its other restorable parts; preserve original files and recovery copies before attempting repair. The personal-store format remains unchanged.

### A second administrator could read or overwrite the local owner's statements

The legacy administrative personal routes derived `LOCAL_USER` after checking only the administrator role. A second administrator's session therefore read the local annotation/history/collections and could mutate them. The new owner guard applies to all methods on the common personal handler boundary. First-administrator/local sessions and the trusted installation token retain compatibility; other administrators use their own `/me` data and can still administer installation work/accounts.

### A captured session could be revoked during preparation and still publish

`locked_update` checked the session before its mutation/projection, but a process-local sign-out could occur during that work while the writer lock remained held. The prepared change was then saved without a final session check. A deterministic test signs out within the operation and proves that both owners' original bytes remain intact.

Native private projections now recheck after CPU/filesystem preparation before admitting a result. Preferences recheck before saving and after preparing their snapshot. History/checkpoint publication repeats authentication after store preparation. Private M3U publication confirms the session before creating its file and before registering the new selection; a newly created unregistered file is cleaned up if registration is refused. Existing files are never removed by that rollback. Shared-folder M3U publication also rechecks its captured session before starting that work.

These guards prevent admission when revocation is already observable at the final check. They do not cancel a filesystem operation already in progress or make logout and an atomic file replacement one cross-resource transaction. No writer lock is held across a network request.

## Verification scope

Fixtures contain only fictional accounts, fake credentials, generic paths and generated personal stores. No existing profile or user store was opened or modified. Bug regressions were observed failing before the corresponding fixes:

- Native/declarative history and merge losing Alice's retained rows after Bob exceeded 500 events.
- A second administrator reading a legacy personal route with HTTP 200 instead of 403.
- Sign-out during `locked_update` returning success instead of refusing publication.
- Private M3U publication returning success despite a failing captured-session confirmation.
- Embedded oversized personal documents being admitted despite the new disk-store byte budget.

Sibling Rust tests additionally cover each owner's retention and counters, 128,000-row global capacity, refused output/input leaving fixture bytes intact, reconcile/save/reread and backup roundtrips, exact JSON formatting/escaping parity, same-named collection isolation, private criteria under search, identical shared catalog responses, reserved achievement-key refusal, owner-private application documents, native HTTP writes and Subsonic declaration batches. Existing account, preference, achievement, M3U, search and Subsonic adversarial tests remain part of targeted verification.

The integrated `tools/check.sh` passes on 2026-10-12 with 2,264 Rust tests (1,970 active unit tests, 291 integration tests and three doctests), none ignored, helper/site checks, required FFmpeg coverage, Rustdoc and the release build. The new report is linked from the README's documentation map. Targeted no-default-feature and fetch-feature server checks also pass. These results do not establish physical audio or other-platform acceptance.

## Limits retained

- The OS account operating the data directory and the installation administrative token remain trusted. Account privacy does not defend against that operator editing files, resetting credentials, reading a whole backup or granting administrative roles.
- Catalog contents, watched-root paths and shared artwork are common installation data. No private library/roots model is implied.
- The store and worker budgets are shared resources. An owner can exhaust an allowed budget and cause subsequent operations to be refused; a refused operation preserves other owners' persisted data. No fairness scheduler or per-account disk partition is introduced.
- Temporary now-playing reports and session/key verification caches are process-local and bounded. They do not retain notes/tags or grant cross-owner access; eviction of a transient report is not deletion of a persisted journal.
- Filesystem canonicalization/private-file protections retain their documented TOCTOU and platform limits. This audit does not claim protection from a hostile process with the same OS access, or Unix permission semantics on Windows.
