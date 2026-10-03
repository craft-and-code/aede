# Subsonic and OpenSubsonic clients

Aède exposes a compatibility adapter at `/rest/<method>` and `/rest/<method>.view`, using the existing catalog, accounts and personal store. It follows the [Subsonic API](https://www.subsonic.org/pages/api.jsp) and the [OpenSubsonic API-key extension](https://opensubsonic.netlify.app/docs/extensions/apikeyauth/). The supported browsing, original audio, sidecar artwork and private personal operations are listed below. Supersonic on macOS is the first validation target; source inspection establishes its request shape, but successful physical playback and complete client interoperability still require a real-client test.

In the route reference, `/rest/{method}` means replacing `{method}` with a supported method name, optionally followed by `.view`.

## Create a client key

Initialize accounts through [accounts](../cli/accounts.md), then run these commands locally with the server's data folder:

```sh
aede accounts keys alice create "Phone"
aede accounts keys alice
aede accounts keys alice revoke <key-id>
```

Creation displays the full `id.secret` key once, after saving it successfully. Listing exposes only `id,label,created_at`; revocation removes that account's selected key. `--json` creates an object containing those metadata fields plus `token`; list/revoke produce an array of metadata. Keys do not expire automatically, survive server restart and are independently revocable. A password/name/role/status change, `accounts revoke` or restoration of credentials invalidates affected keys; restoring accounts rotates the epoch and removes all restored keys. A legacy backup without accounts preserves current keys. Create replacement keys after credential recovery. Each account has at most eight keys and the installation at most 512; labels need non-whitespace text within 128 UTF-8 bytes.

The private account store contains only salted Argon2 verifiers for key secrets. A client may supply `apiKey=<id.secret>`, or use the legacy fields `u=<account-name>&p=<complete-id.secret>` when it lacks an API-key field. The password field carries the revocable client key, not the account password. Hex encoding the complete key as `p=enc:<hex>` is also accepted; it does not encrypt the request. The username must match the key's current account. Account-password login remains unavailable (code 42), as does salted-MD5 `u+t+s` (41). Combining `apiKey` with any old credential parameter returns 43; an invalid/revoked key or mismatched account returns 44. Native bearer sessions remain exclusive to `/api`; they cannot authenticate this adapter, and these keys cannot authenticate native routes.

Use the server's HTTPS URL for remote clients, as explained in [remote access](remote.md). Loopback HTTP remains available for local clients. Host/Origin checks, connection admission and remote work limits still apply. The protocol supplies `apiKey` in its query or form parameters, so keep complete URLs and form bodies out of logs/shared screenshots. The server does not log these requests or secrets. A separate key per client makes revocation practical.

## Requests and responses

Both GET and form POST are supported; POST requires `application/x-www-form-urlencoded`. HEAD is refused with HTTP 405, including on methods that change personal data. For authenticated calls supply `apiKey` (or the legacy key transport above), `v=1.16.1` and a nonempty client name `c`. `f=json` selects JSON; the default is UTF-8 XML with the Subsonic namespace. Replies use `subsonic-response`, protocol version `1.16.1`, server type `Aede`, the actual `serverVersion`, and `openSubsonic=true`. Protocol failures use `status=failed` and `error.code/message`, generally with HTTP 200. Media success uses its own binary content type and HTTP 200/206; an unsatisfiable byte range gives HTTP 416. Parser failures before a format can be selected use XML. JSONP is refused. Every response is `no-store`.

Parameters require valid UTF-8 and percent encoding. Unknown options and duplicate scalar fields, including duplicates across query/body, are refused. Repetition is accepted only for the ID/time lists specified below and preserves order. Query plus form body is limited to 16 KiB; each decoded value to 2048 bytes; at most 32 distinct fields and 256 total field occurrences; body completion to one second. Large playlists therefore need bounded append batches. Browsing and personal work run outside HTTP runtime workers using the two shared catalog slots. Cold key verification uses two bounded password workers and the existing authentication rate limits; a bounded 128-entry fingerprint cache avoids repeated Argon2 work for valid keys. Every call rechecks the current account/key store; media rechecks during transfer. Personal writes additionally recheck permissions under the shared writer lock before an atomic save.

`getOpenSubsonicExtensions` is public, including when accounts are unavailable, and advertises only `apiKeyAuthentication` and `formPost` version 1. It accepts the common protocol parameters but does not require credentials. `tokenInfo` returns the authenticated key's username.

Client protocol versions must use major version 1 and a minor version no newer than 16. The third component must be numeric but does not affect compatibility, following the Subsonic version rules.

## Supported methods

| Method | Extra parameters and behavior |
| --- | --- |
| `ping`, `getLicense`, `tokenInfo` | None. License is valid; no paid license is required. |
| `getUser` | Required `username`, restricted to yourself. Users/admins can stream, download, scrobble and manage their own playlists; auditors have metadata/artwork read access only. Installation administration remains unavailable. |
| `getMusicFolders` | None. One logical library, ID `1`, named Aède; this does not expose filesystem roots. |
| `getArtists` | Optional `musicFolderId=1`; sorted artist indexes including artists without an album. |
| `getArtist`, `getAlbum`, `getSong` | Required opaque `id` from an earlier response. |
| `getAlbumList2` | Required `type`, one of `alphabeticalByName`, `alphabeticalByArtist`, `byYear`, `byGenre`, `random`; optional `size` (default 10, maximum 500), `offset`, `musicFolderId=1`. `byYear` requires `fromYear,toYear`; reversed bounds give descending years. `byGenre` requires `genre`. Those filters are refused for other types. Supersonic's `limit` is a bounded alias for `size`; supplying both is an error. Random pages are reshuffled per request. Other types are refused. |
| `search3` | Required `query`, which may be empty for library synchronization. Optional `artistCount,artistOffset,albumCount,albumOffset,songCount,songOffset` and `musicFolderId=1`. Each count defaults to 20, maximum 1000; offsets default to 0. Counts of zero return an empty category. Search text is at most 1024 UTF-8 bytes. |
| `getGenres` | None; genre names with distinct song/album counts and inherited album genres for otherwise untagged tracks. |
| `getSongsByGenre` | Required `genre`; optional `count` (default 10, maximum 500), `offset`, `musicFolderId=1`. Sorted, independently paged songs; an unknown genre returns an empty list. |
| `getRandomSongs` | Optional `size` (default 10, maximum 500), `genre`, `fromYear`, `toYear`, `musicFolderId=1`. Distinct songs matching all supplied filters; chronological lower bound must not exceed upper bound. |
| `getScanStatus` | None. `scanning` reflects synchronous, HTTP-job and delegated-CLI scan workers, through snapshot publication. `count` is the track count of the last published catalog, not a progress estimate. It does not start a scan. |
| `getCoverArt` | Required dedicated cover `id` from an album/song response; optional `size=1..2048`. Catalogued JPEG/PNG sidecars only. Without `size`, return the verified original bytes. A requested thumbnail is response-only; see artwork below. |
| `stream`, `download` | Required song `id`; exact original binary file. Optional `format=raw`, `maxBitRate=0`, `timeOffset=0`, `converted=false`, `estimateContentLength` set to `true` or `false`. Other conversion/video settings are refused. |
| `getStarred2` | Optional `musicFolderId=1`; your favourite artists/albums/songs only. |
| `star`, `unstar` | At least one repeatable `id`, `albumId` or `artistId`, with the corresponding opaque IDs; update your existing annotations. |
| `setRating` | Required `id,rating`; artist, album or song rating from 0 to 5, with 0 removing your rating. |
| `getPlaylists` | Optional `username`, restricted to yourself; private static playlists only. |
| `getPlaylist` | Required playlist `id`; entries keep order and repetitions. A missing current track fails explicitly and its saved reference remains intact. |
| `createPlaylist` | Repeatable `songId`; required `name` for creation, or `playlistId` to replace an existing private playlist's entries. An empty list is allowed. |
| `updatePlaylist` | Required `playlistId`; optional `name,comment,public=false`, repeatable `songIdToAdd,songIndexToRemove`. Removal indexes address the original list; duplicate/out-of-range indexes are refused. |
| `deletePlaylist` | Required playlist `id`; deletes only your playlist. |
| `scrobble` | Repeatable song `id`, matching optional Unix-millisecond `time` values, and `submission=true` by default. Stores client declarations and increments your private counts. `submission=false` accepts exactly one song and optional time as a temporary now-playing report. |
| `getNowPlaying` | None; current, unexpired reports from your own clients only, checked against still-valid keys. |

Albums are local release editions and songs are track placements, including tracks without albums. Artists' albums come from explicit album artists, including co-artists, rather than composer/guest credits. IDs hash stable Aède references with SHA-256; they survive dense catalog renumbering when the underlying references stay unchanged. Paths, cover paths, raw tags and source prose are omitted. Required album `created` is a documented UTC proxy for the catalog's scan timestamp, since Aède has no album-added timestamp; it is not used to promise a newest-album sort. Browse responses include only your own favourite/rating fields and song play counts, when available.

Audio is passed through byte-for-byte, without DSP, normalization, resampling or tag changes. At most four native/audio/artwork transfers run together; reads use bounded chunks and queues and stop on revocation, shutdown or failed source checks. Audio sources must remain regular catalogued files with the precise size/timestamp captured by a current scan; a changed/legacy-imprecise source requires a rescan. A single `Range: bytes=…` selects original audio bytes. Without a recognized strong validator, `If-Range` causes a full response, protecting resumptions from combining different files. Auditors may browse metadata and artwork but cannot stream or download audio.

Artwork must be an ordinary JPEG/PNG sidecar confined to the release's native folder; source links, escaping paths, corrupt files and inputs beyond the existing image budgets are refused. Validation reads every pixel, under the existing 32 MiB input, 8192-pixel-axis, 16-million-pixel and decoded-memory limits. `size` bounds the longest axis while keeping aspect ratio and never enlarging an image. If reduction is needed, the response contains an in-memory PNG thumbnail; otherwise the original bytes are returned. Source artwork, downloaded 4K images and audio files are never rewritten. Embedded covers and other artwork families are not advertised by this adapter.

## Personal data and listening declarations

Favourites and ratings use the existing annotations. The `starred` date is their last annotation-change time, an explicit proxy because there is no separate favourite timestamp; changing a rating can change that displayed date. Static playlists are separate from query collections. They use stable track references in `user.json`, preserve repeated entries and keep missing references until an explicit edit. Names contain non-whitespace text within 256 UTF-8 bytes; comments are at most 2048 bytes. The store permits 128 playlists per owner, 512 in total, and 2000 ordered entries per playlist. Sharing and selecting another owner are refused, including for administrators. Invalid arguments or unavailable stores never cause a partial save.

Streaming alone creates no listen. A submitted scrobble records a client declaration with its timestamp; it has no fabricated played duration, completion, decoded-audio or acknowledgement evidence. Its recent log retains the newest 500 declarations, including repetitions, while all-time counts outlive the log and also include native acknowledged listens. Timestamps default to receipt time and cannot be more than one day in the future. Submission is not idempotent: replaying a successful request can increment the count again. A now-playing report does not create history or increment counts. One report per key is retained in process memory, with 128 total reports and expiry based on the known track duration (30 seconds to 24 hours; five minutes if unknown). Revocation/account changes hide a report immediately; restart clears all reports.

## First macOS test with Supersonic

Install a macOS build from the [official Supersonic releases](https://github.com/supersonic-app/supersonic/releases), following its [macOS installation instructions](https://github.com/supersonic-app/supersonic#mac-os); the project documents an extra launch step for its non-notarized application. Scan your music, initialize accounts if needed, create a separate client key, and restart the rebuilt server using the same data folder:

```sh
aede accounts keys alice create "Supersonic Mac"
aede serve --port 8787
```

Replace `alice` with your existing account name. From this repository, the verification build produces `./target/release/aede`: use that path instead of `aede` to test the current executable. If your catalog uses a custom data folder, append the same `--data "/path/to/data"` to both commands. Aède itself provides the compatible server; no separate Subsonic server installation is required.

In Supersonic's server dialog choose **Subsonic**, use `http://127.0.0.1:8787` on this Mac (without `/rest`), enter your Aède account name under **Username**, and paste the entire `id.secret` key under **Password**. Enable **Use legacy authentication**. This exact [client setting](https://github.com/supersonic-app/supersonic/blob/0eb34945c59e1d010ef618a0dedd3bc1edd37908/ui/dialogs/addeditserverdialog.go#L54) sends the key in `p` instead of hashing it into an unsupported MD5 token. Keep transcoding disabled so the client requests original audio.

On the **Albums** page select **Title (A-Z)** in the sort menu. Supersonic's default Recently Added uses `newest`, which Aède explicitly refuses until a real addition timestamp exists; the supported [Title (A-Z) mapping](https://github.com/supersonic-app/supersonic/blob/0eb34945c59e1d010ef618a0dedd3bc1edd37908/backend/mediaprovider/subsonic/albumiterator.go#L93) uses `alphabeticalByName`. Year, genre and random browsing are also supported. Start with an album and a format that the client can decode, then verify artwork, seeking, favourites, playlists and declared play counts. A phone or another computer needs the configured HTTPS address, not this Mac's loopback URL.

## Local macOS setup with Submariner

For [Submariner](https://github.com/SubmarinerApp/Submariner), use the same client-key transport. Its [authentication implementation](https://github.com/SubmarinerApp/Submariner/blob/master/Submariner/SBServer.swift) sends `u+t+s` when **Use Token-Based Authentication** is checked, and `u+p=enc:<hex>` when unchecked. Only the latter can carry an Aède application key.

List your existing accounts, then replace `alice` with an enabled administrator or user account:

```sh
aede accounts list
aede accounts keys alice create "Submariner Mac"
```

Copy the entire key after `API key (shown once):`, including the dot. With `aede serve --port 3412` running on this Mac, fill in the server dialog:

| Field | Value |
| --- | --- |
| Server Name | `Aède`, or another label you choose |
| URL | `http://127.0.0.1:3412` |
| Username | The account name used to create the key, such as `alice` |
| Password | The complete application key, not the account password |
| Use Token-Based Authentication | Unchecked |

The URL has no `/api/v1/status` or `/rest` suffix. If accounts are not initialized, first follow [account setup](../cli/accounts.md); account passwords need at least 15 characters. Use the same `--data` directory for account commands and the server when it is customized. Aède already supplies the server; no separate Subsonic installation is required.

These settings establish the authentication format, not complete Submariner interoperability. Artist/album browsing is available; folder navigation, some album sorts and optional client panels may call unsupported methods. Full Submariner navigation and physical playback still require a real-client test.

## Remaining compatibility work

Unimplemented surfaces include `newest/highest/frequent/recent/starred` album-list types, legacy folder browsing and `getStarred`, queue persistence, rich artist/album-info endpoints, OpenSubsonic playback reports, embedded covers, transcoding, podcasts, radio, video and remote administration. Unsupported calls/options fail explicitly. Supersonic's relevant initial/browse/playback paths were checked against its official source; optional panels that need these missing endpoints can still fail. Full real-client navigation/playback, target NAS capacity and compatibility with individual clients remain to be tested. Original audio must be supported by the selected client. Android Symfonium supports API keys according to its [official release note](https://support.symfonium.app/t/version-11-6-0-beta-1/6358); its [provider guide](https://docs.symfonium.app/wiki/providers/subsonic-opensubsonic-media-provider-configuration/) describes original-file and fast-sync settings, but it has not yet been tested against Aède.
