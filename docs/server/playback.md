# Authenticated PCM playback v1

`GET /api/me/v1/playback` upgrades to a WebSocket for one current catalogued track or an explicitly ordered, finite queue. Supply exactly one `Authorization: Bearer <session>` header, using WS on local HTTP or WSS on [HTTPS](remote.md). Only `user` and `admin` sessions may play, because playback writes private history; an `auditor` receives `403 forbidden`. Legacy administrative tokens cannot authenticate this route, even locally. No query parameters are accepted.

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
| `sample_rate` | Integer 8000–192000 Hz. Omit it to use the first decoded rate, capped at 192000 Hz. The chosen rate stays fixed for the entire queue. |
| `bass`, `treble` | Finite numbers from −12 to +12 dB; omitted values are zero, with flat bypass. |

The server opens the regular file represented by the current catalog and checks its size and precise modification time around decoding and before history publication. Changed sources and older catalogs lacking precise timestamps require a fresh scan. It refuses source links and unknown multichannel layouts. Audio files and tags are never rewritten.

The first successful server text message describes the transmitted format:

```json
{"type":"format","track":"track:…","encoding":"f32le","sample_rate":48000,"channels":2,"duration_ms":180000,"max_unacknowledged_frames":48000}
```

`duration_ms` is the catalog duration or `null`; it is not an authoritative frame count. The decoded layout is mono or stereo; recognized multichannel sources use the shared stereo downmix with LFE omitted. Processing uses the existing `PcmSession`: optional rate conversion, fixed normalization gain, tone/headroom and the final sample guard. The shared ready-gain policy reuses tags and existing track measurements; this transport does not measure new loudness. Album mode inspects the complete requested programme for available album tags or an exact ordered-programme cache, otherwise keeps the decoded level. For the original single-track request, the programme is that single path. Queue album preparation may inspect all requested files before the first format message. It does not substitute track loudness for an unavailable album measurement. Processing does not certify bit-perfect output or a physical true-peak ceiling. The client should display the selected settings instead of describing processed PCM as the original encoded file.

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

The server then closes the socket. A zero-length or sub-millisecond listen closes without a `recorded` message because it records no history. The `track` request preserves this single-track lifecycle. To join tracks without reconnecting, opt into the finite queue extension below. Seeking, live queue editing, repeat/shuffle commands and persistent queues remain unavailable in this transport.

## Finite queues and continuous joins

Send `tracks` instead of `track` to opt into the queue extension:

```json
{"type":"start","tracks":["track:…","track:…"],"normalize":"off","sample_rate":48000,"bass":0,"treble":0}
```

Exactly one of `track` and `tracks` is required. `tracks` contains between 1 and 64 current catalog references, in playback order, and the complete control message must still fit within 4 KiB. References are resolved before production; arbitrary paths and unavailable references are refused. Repeated references are intentional separate occurrences. Settings apply to the whole queue; the server does not reorder or persist it.

The first `format` additionally contains `index: 0` and `start_frame: 0`. Queue mode sends these boundary messages for each zero-based occurrence:

```json
{"type":"track","index":0,"track":"track:…","start_frame":0,"duration_ms":180000}
{"type":"track_end","index":0,"track":"track:…","end_frame":8640000}
```

Boundaries describe the cumulative processed output timeline, including delayed rate-converter frames. A `track` marker precedes that occurrence's first PCM; `track_end` follows its final PCM. Catalog duration remains informative only. Binary blocks stay aligned to complete frames, and acknowledgements never reset between tracks. A marker announces a position in the buffered timeline, not that the client's device has already reached it. Display the active track from actual consumption and these boundaries.

Compatible natural joins retain the same `PcmSession`, rate converter and tone filters, flushing the converter only at the end of the compatible group. No silence is inserted and no acknowledgement round trip is required at a compatible boundary. A source-format change flushes the old group and starts new processing; it is not promised to be sample-continuous. The chosen output rate remains fixed. If mono/stereo output changes, the server waits for acknowledgement of the previous format's final frames before sending a new `format` with its occurrence `index` and cumulative `start_frame`. Consume the old format before reconfiguring the output, and retain the global acknowledgement counter.

There is one final `eof` for the whole queue. After consuming and acknowledging it, wait for a `recorded` confirmation for each saved occurrence; queue confirmations additionally contain `index` and `track`. Occurrences below one millisecond have no listening record. Keep the client output open across compatible joins and provision a bounded buffer with enough scheduling margin. Continuous server PCM does not guarantee that a client device or an overloaded host is gapless.

## Listening history

The single-track request records at most one `Play`; a queue records at most one per occurrence, using the existing personal-history rule and the authenticated owner. `ms_played` is acknowledged output frames within that occurrence's boundaries divided by output sample rate, rounded down to milliseconds. Less than one millisecond records nothing. `completed` requires a valid decoded end for that occurrence and acknowledgement of all its assigned output. Sending bytes alone never records a complete listen. Closing during the second track can therefore preserve a completed first listen and an incomplete second listen, without counting later queued tracks.

Queue records are held in bounded memory and saved as a batch when playback ends or disconnects, outside decoding and network production. They are not durably saved at every boundary; a process crash before publication can lose those pending listens. Each source is revalidated separately under the shared store lock: an invalidated occurrence cannot be saved, while earlier valid occurrences can still be saved with a reported history failure. Revocation or loss of account permissions prevents late batch publication. A final confirmation lost in transit does not establish which writes failed.

A normal early close, timeout or processing failure can record acknowledged partial audio as one incomplete listen, provided the source and session still permit publication. Revocation or changed/unavailable source identity prevents a late write. Acknowledgements are client declarations, not proof that a human heard the sound. Do not also POST this same listen to the explicit history route: that route records independent events and cannot deduplicate a separately submitted client report. Losing the final confirmation does not prove the write failed; do not blindly replay a history submission.

## Bounds and errors

Four playback slots bound active decoding and associated history work. Additional upgrades return `503 playback_busy`; invalid sessions return `401 unauthorized`, and auditor access returns `403 forbidden`. Control messages are limited to 4 KiB and finite queues to 64 occurrences. The producer has four queued events and the client window is one second. Initial start/source preparation, missing client consumption progress and stalled source production have ten-second deadlines; socket sends have a five-second deadline. A paused client must resume within the consumption deadline or reconnect. History publication has a bounded response wait and retries a busy data lock for at most five seconds before failing.

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
