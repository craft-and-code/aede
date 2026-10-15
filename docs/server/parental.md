# Account parental controls

The `parental_controls` capability adds administrator-owned artist restrictions and listening budgets to authenticated accounts. It does not change the shared physical catalogue or edit audio/tags. Ordinary anonymous local compatibility mode is unchanged. Administrators cannot be supervised. This is account-level enforcement in Aède and cooperating native clients, not an operating-system lock or a restriction on files/accounts outside Aède.

## Policy administration

These routes require a current administrator account session over HTTPS or local HTTP. Enabling rules claims an exclusive lock for that stable administrator identity. Other administrators may read the policy but cannot change the rules, rename/promote/disable the protected account, reset its password or revoke its sessions. The supervising administrator releases the lock by disabling the rules. Other administrators also cannot reset or alter that supervisor while it owns active locks, preventing impersonation through a password reset. An administrator with active locks cannot be disabled or demoted until those locks are released. A legacy local token cannot claim or override a named lock. This is an API/account boundary, not protection against a trusted operator rewriting installation files or restoring backups through the local CLI.

| Method and route | Body | Result |
| --- | --- | --- |
| `GET /api/admin/v1/accounts/{username}/parental` | None | `{owner,revision,locked_by,policy}` |
| `PUT /api/admin/v1/accounts/{username}/parental` | `{expected_owner,expected_revision,policy}` | Saved `{owner,revision,locked_by,policy}` |
| `GET /api/me/v1/parental` | None | Current owner's status |
| `POST /api/me/v1/parental/lease` | External playback action below | Grant or 204 |

The stable `owner` and revision returned by GET must both be sent back when saving. A renamed/replaced account or concurrent policy edit returns `409 parental_conflict`; the client must reload rather than silently target the replacement. Unknown accounts return 404. An administrator target, unknown fields, invalid references, excessive limits and unknown time zones are refused. The login response's `account` and own session metadata always include `parental_enabled`; clients must not infer this solely from a role.

`policy` has exactly these fields:

```json
{
  "enabled": true,
  "timezone": "Europe/Paris",
  "blocked_artists": ["artist:example artist"],
  "radio_allowed": true,
  "podcasts_allowed": true,
  "blocked_podcasts": ["https://example.invalid/podcast.xml"],
  "days": [
    {"windows": [{"start_minute": 480, "end_minute": 1200}], "limit_seconds": 3600},
    {"windows": [], "limit_seconds": null},
    {"windows": [], "limit_seconds": null},
    {"windows": [], "limit_seconds": null},
    {"windows": [], "limit_seconds": null},
    {"windows": [{"start_minute": 0, "end_minute": 1440}], "limit_seconds": 7200},
    {"windows": [{"start_minute": 0, "end_minute": 1440}], "limit_seconds": 7200}
  ],
  "week_limit_seconds": 10800
}
```

Days run Monday through Sunday. Clock windows are half-open local minutes, at most eight non-overlapping windows per day. Split an overnight window across its two days. An empty window list permits no listening that day. `null` is an unlimited budget; zero permits none. Daily limits cannot exceed 86400 seconds, weekly limits 604800 seconds. The IANA time zone governs civil dates and Monday-to-Monday weeks, including daylight-saving transitions. Up to 256 unfederated artist references and 128 canonical HTTP/HTTPS podcast RSS addresses are accepted. Disabling a policy retains its configuration and usage and clears `locked_by`. Old stores without a lock remain unclaimed until a named administrator saves enabled rules; omitted `podcasts_allowed` retains the historical allowed default. A global podcast refusal blocks all feeds; otherwise `blocked_podcasts` refuses only those addresses. Radio and podcast menus are hidden by cooperating clients when their category is forbidden. Ownership or stale-revision conflicts cannot be bypassed by replacing the username.

Own status is `{owner,revision,policy,day_used_seconds,week_used_seconds,remaining_seconds,allowed,reason,next_change_at}`. Remaining seconds are null without a budget. `next_change_at` is a Unix-seconds calendar transition, not a playback position. Reasons are `outside_schedule`, `daily_limit`, `weekly_limit`, `radio_blocked`, `podcast_blocked`, `artist_blocked` or `policy_unavailable`; successful status has a null reason. Music status does not test a particular artist: source selection does that separately. Browsing remains possible outside listening hours.

## Visibility and private data

Before any catalogue projection, a supervised request uses a compact, remapped catalogue subgraph excluding blocked artists and their affected releases/tracks/recordings/credits. List counts, direct entity selection, artwork, lyrics, waveforms, advanced search and personal selection therefore use the same visible graph. A stable reference does not bypass the filter. Two cached projections bound retained graphs; catalogue publication, effective blocked references and policy revisions invalidate/reselect them. PCM source transitions reuse this cache rather than rebuilding the graph for each track or crossfade voice.

Personal storage is reconciled with the complete catalogue before the restricted view is applied. Existing private tags, notes, stars, favourites, collections and history are retained; hidden history/annotations do not appear while blocked, and become visible again if permitted. Another account's data is never loaded as the current owner. Each request rechecks captured credentials and effective policy before publication. A policy change invalidates an in-flight old view or stream rather than returning stale access.

Root inventories, whole-library diagnostics, catalogue/task event streams, shared M3U-file inventories and original private-M3U downloads are refused for supervised accounts because they can expose unfiltered names/paths or bypass controlled playback. Owner-private M3U summaries/playback selections require all referenced tracks to remain visible. Subsonic browsing uses the restricted graph, but original `stream`/`download` and playlist retrieval are unavailable for supervised accounts: their raw transport cannot enforce consumed-time quotas. A client must not silently fall back to that transport or an external/HQPlayer output.

## Controlled listening

Native PCM reserves a short lease only after its format is ready, before transmitting audio. Reservations are at most ten seconds and stop at a schedule/budget boundary. Live credentials, policy and windows are rechecked; stream completion, stop, seek, disconnect and failures settle the current reservation. Already transmitted PCM remains charged when closing, even without an acknowledgement; up to one second of received but unplayed prebuffer may therefore count. Supervised PCM is paced to at most one second ahead of real time and never beyond its grant, so dishonest rapid acknowledgements cannot download an unlimited track. Existing unsupervised PCM streams are stopped if the account becomes supervised.

For direct radio and podcast audio, a cooperating native client must reserve before starting, renew while playing and release on pause/stop/logout:

```json
{"action":"reserve","media":{"kind":"radio"},"seconds":10}
{"action":"reserve","media":{"kind":"podcast","feed":"https://example.invalid/podcast.xml"},"seconds":10}
{"action":"renew","lease_id":"64 hexadecimal characters","consumed_ms":1000,"seconds":10}
{"action":"release","lease_id":"64 hexadecimal characters","consumed_ms":1000}
```

Reserve/renew return `{lease_id,revision,expires_at,granted_seconds,server_now_ms}`. Expiry is Unix seconds; the server clock is Unix milliseconds at response construction. Derive a conservative local monotonic deadline from the request start and the smaller of the grant duration and `expires_at * 1000 - server_now_ms`; do not rely on a matching client wall clock. Each renewed grant resets its cumulative `consumed_ms` counter. Release returns 204. Music leases cannot be requested through this external endpoint. Lease IDs are server-generated and owner-bound; a different account cannot renew or release one. Podcast identity is canonicalized by the shared core (scheme/host/default ports, path segments, safe percent encodings and fragments); a URL spelling variant cannot bypass the blacklist.

Budgets count the union of active listening intervals, so overlapping crossfade voices do not double-charge the same time. A client cannot refund already elapsed active lease time by reporting zero consumption; release refunds only unused future time. An interrupted process conservatively retains outstanding reservations. Expired lease identities remain within the eight-slot bound until admission needs a slot or retention forgets them, so renewing one voice does not invalidate another. The 32-day retained interval window includes up to ten seconds of already prepaid audio; at most that much of the oldest past is pruned earlier, outside current daily and weekly budgets. At most eight active leases per owner bound concurrency. A short reserve/start/network delay may therefore count toward listening time; quotas are not a forensic measurement of sound reaching a physical device.

Admission/refusal errors use the ordinary envelope with `parental_outside_schedule`, `parental_daily_limit`, `parental_weekly_limit`, `parental_radio_blocked`, `parental_podcast_blocked`, `parental_artist_blocked`, `parental_policy_unavailable` or `parental_conflict`. Administrative lock refusal uses `parental_locked`; enabling through an unnamed legacy token uses `parental_administrator_required`. PCM errors use the same codes. Missing/corrupt protected policy data fails closed after the running process has observed it; an administrator can still authenticate for recovery. Initial absence means no configured policies. Private backups include parental policies/usage; restoring older backups without that part preserves existing restrictions.
