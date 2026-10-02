# Continuous decoding and album joins

An album transition can contain intended silence or deliberately continuous music. **Gapless** means adding/removing no unintended audio at the join, not deleting all silence found in files. Compressed formats can include encoder delay/padding that must be interpreted before joining playable frames.

## What is implemented

The progressive decoder supplies finite, complete PCM frames without decoding the whole selection first. Real fixtures verify playable frame counts for FLAC, WAV, LAME MP3 and native Vorbis. The decoder trims declared MP3 encoder delay/padding and Opus pre-skip/end boundaries. Optional FFmpeg decoding handles Opus and AAC/ALAC in M4A. Corrupt/truncated bounds, detected chained Vorbis resets and missing valid end bounds are errors rather than invented frame counts.

Matching output formats reuse a native stream or the ffplay process. A shared continuous processing session preserves compatible tone and rate-conversion state at natural track joins, rounds cumulative frame boundaries, and attributes delayed converter output to the original track/gain. The converter tail is flushed once at group end. A real FLAC-to-MP3 fixture verifies exactly concatenated PCM without inserted frames.

## When a fresh session is needed

Input/output/tone incompatibility starts a new processing session. An output-format change reopens the sink. Skip, Previous, Stop and failed sources discard pending processing state instead of leaking old audio into the next selection. Going naturally to the next compatible track is different from explicitly skipping.

```sh
aede play "A Continuous Album" --normalize album
```

Use a catalogued album selection to preserve its order and album normalization. On Unix terminals, Space pauses/resumes, `n`/Right moves forward, `p`/Left goes back or restarts after three seconds, and `q` stops. Windows terminal transport controls are future work. Current CLI playback does not drive the separate in-memory queue's repeat/shuffle capabilities.

## Hardware boundaries

The native ring has a bounded 500 ms capacity at the negotiated rate, not unlimited buffering. Slow file opening/decoding can exhaust it and cause underrun silence. Device buffering, scheduler delays and format reopening can also affect audible joins. **Physical gapless playback has not been measured on hardware**, including processed joins. Software PCM continuity is valuable evidence, not a promise about every device.

Final output drain observes consumed-frame progress and polls controls, with an approximate active-time host allowance. It is not physical acknowledgement that the last sample emerged from a DAC. See [Output diagnostics](output.md) and the [technical join/drain contract](../design/playback.md).

The animation illustrates intended joins, trimming and resets; it does not play or verify a real album.
