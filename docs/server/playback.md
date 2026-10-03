# Authenticated PCM playback v1

`GET /api/me/v1/playback` upgrades to a WebSocket for one current catalogued track. Supply exactly one `Authorization: Bearer <session>` header, using WS on local HTTP or WSS on [HTTPS](remote.md). Only `user` and `admin` sessions may play, because playback writes private history; an `auditor` receives `403 forbidden`. Legacy administrative tokens cannot authenticate this route, even locally. No query parameters are accepted.

This is a native client contract. The client owns its audio device, clock and consumption counter. Aède does not open a server audio device, supply a player UI, accept arbitrary file paths or implement Subsonic here. Browser WebSocket clients cannot set this bearer header directly; a browser login/transport design remains separate work.

## Start and format

Send one text JSON object within ten seconds of the upgrade:

```json
{"type":"start","track":"track:…","normalize":"track","bass":0,"treble":0}
```

Copy `track` from a catalog response's stable reference. Unknown fields, duplicate fields, other message types and binary control messages are refused. Optional settings are:

| Field | Accepted values and default |
| --- | --- |
| `normalize` | `off`, `track` or `album`; default `track`. |
| `sample_rate` | Integer 8000–192000 Hz. Omit it to retain the decoded rate, capped at 192000 Hz. |
| `bass`, `treble` | Finite numbers from −12 to +12 dB; omitted values are zero, with flat bypass. |

The server opens the regular file represented by the current catalog and checks its size and precise modification time around decoding and before history publication. Changed sources and older catalogs lacking precise timestamps require a fresh scan. It refuses source links and unknown multichannel layouts. Audio files and tags are never rewritten.

The first successful server text message describes the transmitted format:

```json
{"type":"format","track":"track:…","encoding":"f32le","sample_rate":48000,"channels":2,"duration_ms":180000,"max_unacknowledged_frames":48000}
```

`duration_ms` is the catalog duration or `null`; it is not an authoritative frame count. The decoded layout is mono or stereo; recognized multichannel sources use the shared stereo downmix with LFE omitted. Processing uses the existing `PcmSession`: optional rate conversion, fixed normalization gain, tone/headroom and the final sample guard. The shared ready-gain policy reuses tags and existing track measurements; this single-track socket neither measures new loudness nor loads a complete album programme. Album mode uses available album tags or an exact single-path programme cache, otherwise keeps the decoded level. It does not substitute track loudness for an unavailable album measurement. Processing does not certify bit-perfect output or a physical true-peak ceiling. The client should display the selected settings instead of describing processed PCM as the original encoded file.

## Audio and acknowledgements

Each binary message contains complete interleaved frames of IEEE 754 little-endian `f32`, at most 32 KiB. One frame contains one sample per channel, so its byte length is `channels × 4`. Preserve message order and channel alignment. Binary message boundaries have no musical meaning.

The client sends cumulative text acknowledgements only after its output consumes those frames:

```json
{"type":"ack","frames":4096}
```

`frames` is a non-negative integer, never decreases and never exceeds the frames already received. Repeating an acknowledgement does not count a second listen or keep a stalled stream alive. Aède allows at most one second of sent but unacknowledged PCM, exposed by `max_unacknowledged_frames`; it pauses production as this window fills. A growing valid acknowledgement counts as authenticated client activity; server sends and ping/pong do not prolong session inactivity. Absolute session expiry and revocation still apply.

At decoded EOF, the server sends:

```json
{"type":"eof","frames":8640000}
```

The final `frames` includes the processed stream's converter tail. Consume and acknowledge all remaining frames, then wait for the saved-history confirmation:

```json
{"type":"recorded","ms_played":180000,"completed":true}
```

The server then closes the socket. A zero-length or sub-millisecond listen closes without a `recorded` message because it records no history. Reconnect and send a new start to play another track. Queue control, seeking and continuous remote joins are not part of v1.

## Listening history

One socket records at most one `Play` using the existing personal-history rule and the authenticated owner. `ms_played` is acknowledged output frames divided by output sample rate, rounded down to milliseconds. Less than one millisecond records nothing. `completed` requires valid decoded EOF and acknowledgement of every sent frame. Sending bytes alone never records a complete listen.

A normal early close, timeout or processing failure can record acknowledged partial audio as one incomplete listen, provided the source and session still permit publication. Revocation or changed/unavailable source identity prevents a late write. Acknowledgements are client declarations, not proof that a human heard the sound. Do not also POST this same listen to the explicit history route: that route records independent events and cannot deduplicate a separately submitted client report. Losing the final confirmation does not prove the write failed; do not blindly replay a history submission.

## Bounds and errors

Four playback slots bound active decoding and associated history work. Additional upgrades return `503 playback_busy`; invalid sessions return `401 unauthorized`, and auditor access returns `403 forbidden`. Control messages are limited to 4 KiB. The producer has four queued audio blocks and the client window is one second. Initial start/source preparation, missing client consumption progress and stalled source production have ten-second deadlines; socket sends have a five-second deadline. A paused client must resume within the consumption deadline or reconnect. History publication has a bounded response wait and retries a busy data lock for at most five seconds before failing.

After upgrade, failures send `{"type":"error","code":"…","message":"…"}` when the socket remains writable, then close. Inspect `code`; human messages may change:

| Code | Meaning |
| --- | --- |
| `invalid_start`, `invalid_ack` | Invalid control message, setting, reference kind or consumption counter. |
| `track_not_found`, `catalog_unavailable` | Refresh the catalog/reference before retrying. |
| `source_changed`, `source_unavailable` | Restore or rescan the regular audio source. |
| `decode_failed`, `processing_failed`, `stream_failed` | Decode, DSP or worker failure; preserve partial-listen semantics. |
| `ack_timeout` | Client consumption made no progress in time. |
| `authentication_expired` | Session expired/revoked or account no longer permits playback. |
| `history_failed` | History could not be confirmed as saved. |
| `server_shutdown` | Server is stopping. |

Transport and closed-client failures cannot guarantee a JSON error or final confirmation. The shared [account](accounts.md) and [HTTPS](remote.md) restrictions apply to the handshake and the live stream. These automated PCM/transport checks do not establish physical gapless playback, client-device quality or target-NAS capacity.
