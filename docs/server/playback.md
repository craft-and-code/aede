# Authenticated PCM playback v1

`GET /api/me/v1/playback` upgrades to a WebSocket for one current catalogued track or an explicitly ordered, finite queue. Supply exactly one `Authorization: Bearer <session>` header, using WS on local HTTP or WSS on [HTTPS](remote.md). Only `user` and `admin` sessions may play, because playback writes private history; an `auditor` receives `403 forbidden`. Legacy administrative tokens cannot authenticate this route, even locally. No query parameters are accepted.

This is a native client contract. The client owns its audio device, clock and consumption counter. Aède does not open a server audio device, supply a player UI, accept arbitrary file paths or implement Subsonic here. Browser WebSocket clients cannot set this bearer header directly; a browser login/transport design remains separate work.

## Start and format

Send one text JSON object within ten seconds of the upgrade:

```json
{"type":"start","track":"track:…","normalize":"track","bass":0,"treble":0}
```

Copy `track` from a catalog response's stable reference. Unknown fields, duplicate fields, undocumented message types and binary control messages are refused. The interactive extension below additionally defines initial `resume` and runtime controls. Optional settings are:

| Field | Accepted values and default |
| --- | --- |
| `normalize` | `off`, `track` or `album`; default `track`. |
| `sample_rate` | Integer 8000–192000 Hz. Omit it to use the first decoded rate, capped at 192000 Hz. The chosen rate stays fixed for the entire queue. |
| `bass`, `treble` | Finite numbers from −12 to +12 dB; omitted values are zero, with flat bypass. |

The server opens the regular file represented by the current catalog and checks its size and precise modification time around decoding and before history publication. Changed sources and older catalogs lacking precise timestamps require a fresh scan. Catalogued source paths must be absolute and contain no parent-directory traversal (`..`); malformed paths are refused rather than resolved against the server's working directory. It refuses source links and unknown multichannel layouts. Audio files and tags are never rewritten.

For a FLAC carrying a nonzero STREAMINFO MD5, reaching decoded EOF also checks the complete integer PCM against that digest, before floating-point conversion, normalization, downmix or other DSP. A mismatch ends playback with `decode_failed`, without a successful `eof` or `track_end` for the failed occurrence. Earlier PCM may already have reached the client: any accepted acknowledgements can still become an incomplete listen. Previously completed and fully acknowledged occurrences are preserved; later queued tracks are not started. A FLAC without a stored digest remains playable, but no MD5 verification is claimed. Closing before decoded EOF cannot establish the complete digest. This checks the encoded source's decoded content, not its identity or the client's physical output, and does not replace the source-identity checks.

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

The server then closes the socket. A zero-length or sub-millisecond listen closes without a `recorded` message because it records no history. The `track` request preserves this single-track lifecycle. To join tracks without reconnecting, opt into the finite queue extension below. Seeking, future-tail editing and persistent resume require the separate optional interactive extension; repeat/shuffle commands remain unavailable.

## Finite queues and continuous joins

Send `tracks` instead of `track` to opt into the queue extension:

```json
{"type":"start","tracks":["track:…","track:…"],"normalize":"off","sample_rate":48000,"bass":0,"treble":0}
```

Exactly one of `track` and `tracks` is required. `tracks` contains between 1 and 64 current catalog references, in playback order, and the complete control message must still fit within 4 KiB. References are resolved before production; arbitrary paths and unavailable references are refused. Repeated references are intentional separate occurrences. Settings apply to the whole queue; this original finite mode does not reorder or persist it.

The first `format` additionally contains `index: 0` and `start_frame: 0`. Queue mode sends these boundary messages for each zero-based occurrence:

```json
{"type":"track","index":0,"track":"track:…","start_frame":0,"duration_ms":180000}
{"type":"track_end","index":0,"track":"track:…","end_frame":8640000}
```

Boundaries describe the cumulative processed output timeline, including delayed rate-converter frames. A `track` marker precedes that occurrence's first PCM; `track_end` follows its final PCM. Catalog duration remains informative only. Binary blocks stay aligned to complete frames, and acknowledgements never reset between tracks. A marker announces a position in the buffered timeline, not that the client's device has already reached it. Display the active track from actual consumption and these boundaries.

Compatible natural joins retain the same `PcmSession`, rate converter and tone filters, flushing the converter only at the end of the compatible group. No silence is inserted and no acknowledgement round trip is required at a compatible boundary. A source-format change flushes the old group and starts new processing; it is not promised to be sample-continuous. The chosen output rate remains fixed. If mono/stereo output changes, the server waits for acknowledgement of the previous format's final frames before sending a new `format` with its occurrence `index` and cumulative `start_frame`. Consume the old format before reconfiguring the output, and retain the global acknowledgement counter.

There is one final `eof` for the whole queue. After consuming and acknowledging it, wait for a `recorded` confirmation for each saved occurrence; queue confirmations additionally contain `index` and `track`. Occurrences below one millisecond have no listening record. Keep the client output open across compatible joins and provision a bounded buffer with enough scheduling margin. Continuous server PCM does not guarantee that a client device or an overloaded host is gapless.

## Interactive queues, seeking and persistent resume

The interactive extension is optional. Existing single-track and finite-queue requests keep their original messages and cumulative counters. To enable controls, add `interactive: true` to `start`; this mode uses queue semantics even with one `track`:

```json
{"type":"start","tracks":["track:…","track:…"],"interactive":true,"profile":"desktop","normalize":"off"}
```

`profile` is optional and enables a private persistent checkpoint. It contains 1–64 ASCII letters, digits, underscores or hyphens. Without it, the interactive queue remains transient. `profile` is accepted only with interactive mode. Only one active socket may own a given account/profile; another receives `state_conflict`, while distinct profiles remain independent. A profile belongs to the authenticated account; it is neither an account name nor a shared playlist. The same track reference may occur more than once, with a separate stable occurrence ID for each queue entry.

Interactive messages identify an output `epoch` and queue `revision`. Each connection begins at epoch 0; a new profile starts at revision 1, while reusing/resuming a profile advances its saved revision. The initial `queue` message supplies the authoritative queue, revision, current occurrence and source position:

```json
{"type":"queue","epoch":0,"revision":1,"items":[{"occurrence":1,"track":"track:…"},{"occurrence":2,"track":"track:…"}],"current_occurrence":1,"position_ms":0}
```

Keep these occurrence IDs rather than deriving them from the queue index or track reference. Interactive `format`, `track`, `track_end` and `eof` messages also carry `epoch`/`revision`; occurrence-specific messages additionally carry `occurrence`. `format` and `track` carry `position_ms` for the source offset of the new segment. Acknowledgements include the current epoch, and their frame counter is cumulative **within that epoch**:

```json
{"type":"ack","epoch":0,"frames":4096}
```

A command carries the frames actually consumed when it was issued. This makes consumption and the requested change one operation; never include buffered frames that will be discarded. Examples:

```json
{"type":"seek","epoch":0,"frames":4096,"position_ms":30000}
{"type":"edit_queue","epoch":0,"frames":4096,"revision":1,"items":[{"occurrence":2},{"track":"track:…"}]}
{"type":"stop","epoch":0,"frames":4096}
```

A seek chooses an absolute, non-negative millisecond position within the current consumed occurrence, at most 24 hours. A request beyond the actual decoded end is refused with `invalid_seek`; the exact end advances naturally to the next occurrence. Progressive decoding remains subject to the source-production deadline, so a very long prefix may fail explicitly rather than monopolize a worker. It progressively decodes the source prefix and restarts processing at the requested position; it is not a byte-range operation. A visit affected by seeking stays an incomplete listen even if decoding later reaches EOF. Listening duration counts acknowledged audio across its segments, without counting the skipped prefix.

`edit_queue.items` replaces only the future tail: the consumed prefix and current occurrence are retained. Reuse an existing future `occurrence` to move it; omit it to remove it. A `track` creates a new occurrence from a current catalog reference. Existing occurrence IDs must be future entries and must not appear twice in the replacement tail. New track references may repeat intentionally. The complete resulting queue is limited to 64 occurrences. `revision` protects against editing an obsolete queue snapshot. To skip the current occurrence, seek to its end; removing it through a future-tail edit is refused.

A successful seek or edit discards prefetched old PCM, advances `epoch` and returns a reset boundary:

```json
{"type":"reset","epoch":1,"revision":2,"items":[{"occurrence":1,"track":"track:…"},{"occurrence":2,"track":"track:…"}],"current_occurrence":1,"position_ms":30000}
```

The client must stop using every old-epoch frame, clear unconsumed device/application buffers, reset its cumulative frame counter to zero and acknowledge the boundary:

```json
{"type":"reset_ack","epoch":1}
```

Only then does the server send the new format/track position and PCM. The returned source offset is part of the new playback clock: epoch frames start at zero, but the track may start midway. Old-epoch acknowledgements and commands are refused. Every successful seek or edit increments both the epoch and revision. Complete `reset_ack` within ten seconds; no new PCM is sent before it, and authentication/revocation/shutdown remain checked during the wait. Tail edits may therefore interrupt current playback while prefetched audio is replaced. Only unchanged, naturally advancing compatible joins retain continuous processing; a user control is not promised to be gapless. `stop` flushes acknowledged listening/checkpoint state and ends the socket.

### Explicit resume and checkpoint durability

Reconnect with a new valid account session, then send this as the initial message:

```json
{"type":"resume","profile":"desktop"}
```

Resume restores the saved queue, current occurrence, acknowledged source position and playback settings. It does not accept a new track or replacement settings. The server validates every saved source against the current catalog, file size and precise modification time; missing or changed sources require a fresh scan and a new start instead of silently playing a same-name file. An ordinary server restart invalidates bearer sessions, so login again before requesting resume. No playback starts automatically when the server restarts. A fully consumed queue is saved with `current_occurrence: null` and `position_ms: 0`; resuming it announces the saved queue, confirms its completed state and closes without restarting its first track.

A resumed connection starts a new listen. Audio acknowledged on the previous socket is not counted again. An occurrence resumed after its beginning stays incomplete because its earlier prefix was heard in another connection; an occurrence saved at position zero can complete if this new connection acknowledges its entire audio. Following untouched occurrences can complete normally. A checkpoint is a cursor/settings record, not proof of listening and not an additional `Play` event. Cursor positions are rounded down to milliseconds; reopening after an edit or resume may replay less than one millisecond of source audio. A failed or unconfirmed seek preserves the last validated cursor.

Growing acknowledgements schedule a checkpoint at most every five seconds. The bounded background writer keeps disk work away from PCM production; successful controls and normal EOF/disconnection request a final flush. A busy store lock may delay publication. A process crash can lose the latest five seconds plus any pending write delay, and can lose listening history held in memory; it never invents consumption to fill that gap. A saved checkpoint does not make every accepted acknowledgement durable. Checkpoints live in the private personal `user.json` store and are included in private backups; catalog reset preserves them. Account deletion makes their owner unavailable. Controls/final flushes wait at most ten seconds, while an individual busy store-lock acquisition retries for at most five seconds. Publication failure is explicit.

At normal termination, interactive listening confirmations use stable `occurrence` plus `track`, `ms_played` and `completed`. After those confirmations, wait for the saved-state result before claiming the final checkpoint was saved:

```json
{"type":"saved","profile":"desktop","revision":2,"completed":false}
```

`completed` means the remaining queue reached acknowledged EOF; the individual `recorded` confirmations describe whether each visit was complete. For a transient interactive queue, `profile` is `null`; no durable resume state is implied. Loss of this final confirmation leaves write status uncertain and must not cause duplicate history submissions.

The server retains at most 16 profiles per owner and 512 overall, with no automatic eviction of another profile. Each interactive connection accepts at most 128 controls and keeps at most 256 listened visits. These limits bound memory, decoder restarts and persistence. The client must handle refusal explicitly instead of retrying controls indefinitely. Repeat and shuffle remain client decisions over a bounded queue; this extension adds no server repeat/shuffle command.

## Lyrics and the client clock

Retrieve local lyrics separately with `GET /api/v1/lyrics?track=<stable reference>` and the catalog's session authentication. The [lyrics API](../api.md#local-lyrics) supplies complete text, provenance category and optional millisecond timestamps. It does not inject lyrics into PCM, alter audio flow control or create listening history. No Subsonic/OpenSubsonic lyrics method is added by this native endpoint.

Fetch once per current track, then follow the position of audio actually consumed by the client output. Receipt of audio, a `track` marker or a lyric response does not mean the device has reached that position. For a finite PCM queue, map the cumulative consumed frame position to the appropriate occurrence using `start_frame`/`end_frame`, subtract that occurrence's start and convert with the transmitted sample rate. If the client resamples, map device consumption back to transmitted frames first. A repeated track starts its lyric position again; its next occurrence may reuse the same source data.

Group equal timestamps in source order and sort the groups chronologically. Select the latest group whose `at_ms` does not exceed the track position; display nothing before the first timed group. A timed empty group clears the preceding words, and the final group remains active through track end unless an empty cue clears it. LRC offset is already applied; do not apply it again. Pause freezes the clock. A client that can seek through another audio transport recalculates the current group after every move rather than replaying intervening lines. For interactive PCM, add the returned source offset to the new epoch’s consumed track position and recalculate after each reset; discarded old PCM never advances lyrics.

Mixed timed/untimed lyrics keep their complete source text: only timed lines drive highlighting, while plain text stays available for reading. `lyrics: null` means no nonempty local lyrics; an unavailable, stale or excessive source is an explicit error. These optional client rules also appear in [Compatible Aède](compatible-aede.md#optional-synchronized-lyrics).

## Listening history

The single-track request records at most one `Play`; a queue records at most one per listened occurrence in that connection, using the existing personal-history rule and the authenticated owner. Interactive resets aggregate acknowledged frames before rounding the visit duration once, including segments shorter than a millisecond; a seek makes its visit incomplete. A resumed connection starts a separate visit instead of replaying the preceding connection’s history; a nonzero resumed source offset keeps that visit incomplete. `ms_played` is acknowledged output frames within that occurrence's boundaries divided by output sample rate, rounded down to milliseconds. Less than one millisecond records nothing. `completed` requires a valid decoded end for that occurrence and acknowledgement of all its assigned output. Sending bytes alone never records a complete listen. Closing during the second track can therefore preserve a completed first listen and an incomplete second listen, without counting later queued tracks.

Queue records are held in bounded memory and saved as a batch when playback ends or disconnects, outside decoding and network production. They are not durably saved at every boundary; a process crash before publication can lose those pending listens. Each source is revalidated separately under the shared store lock: an invalidated occurrence cannot be saved, while earlier valid occurrences can still be saved with a reported history failure. Revocation or loss of account permissions prevents late batch publication. A final confirmation lost in transit does not establish which writes failed.

A normal early close, timeout or processing failure can record acknowledged partial audio as one incomplete listen, provided the source and session still permit publication. Revocation or changed/unavailable source identity prevents a late write. Acknowledgements are client declarations, not proof that a human heard the sound. Do not also POST this same listen to the explicit history route: that route records independent events and cannot deduplicate a separately submitted client report. Losing the final confirmation does not prove the write failed; do not blindly replay a history submission.

## Bounds and errors

Four playback slots bound active decoding and associated history work. Additional upgrades return `503 playback_busy`; invalid sessions return `401 unauthorized`, and auditor access returns `403 forbidden`. Control messages are limited to 4 KiB and finite queues to 64 occurrences. The producer has four queued events and the client window is one second. Initial start/source preparation, missing client consumption progress and stalled source production have ten-second deadlines; socket sends have a five-second deadline. A paused client must resume within the consumption deadline or reconnect. History publication has a bounded response wait and retries a busy data lock for at most five seconds before failing.

After upgrade, failures send `{"type":"error","code":"…","message":"…"}` when the socket remains writable, then close. Inspect `code`; human messages may change:

| Code | Meaning |
| --- | --- |
| `invalid_start`, `invalid_ack` | Invalid control message, setting, reference kind or consumption counter. |
| `invalid_control` | Interactive message, epoch, queue revision, occurrence or connection limit is invalid. |
| `invalid_seek` | Requested source position is beyond the decoded end or exceeds the 24-hour bound. |
| `state_failed` | Private checkpoint is absent, invalid, unavailable, over the profile limit or could not be saved/restored. |
| `state_conflict` | Another socket owns this account/profile, or a saved state changed concurrently. |
| `track_not_found`, `catalog_unavailable` | Refresh the catalog/reference before retrying. |
| `source_changed`, `source_unavailable` | Restore or rescan the regular audio source. |
| `decode_failed` | Audio decoding failed, including a FLAC decoded-content MD5 mismatch at EOF; preserve partial-listen semantics. |
| `processing_failed`, `stream_failed` | DSP or worker failure; preserve partial-listen semantics. |
| `ack_timeout` | Client consumption made no progress in time. |
| `authentication_expired` | Session expired/revoked or account no longer permits playback. |
| `history_failed` | History could not be confirmed as saved. |
| `server_shutdown` | Server is stopping. |

Transport and closed-client failures cannot guarantee a JSON error or final confirmation. The shared [account](accounts.md) and [HTTPS](remote.md) restrictions apply to the handshake and the live stream. These automated PCM/transport checks do not establish physical gapless playback, client-device quality or target-NAS capacity.
