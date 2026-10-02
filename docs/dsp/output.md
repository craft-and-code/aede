# Output stages, meters and diagnostics

The sound you hear includes Aède's processing, the output backend, the operating system and the device. Diagnostics identify the part Aède can observe rather than calling the whole path transparent or bit-perfect.

## Native output and fallback

`aede play` uses native CPAL when a compatible channel/sample format is available, preferring floating point. Otherwise it uses ffplay. Opus and M4A decoding also require FFmpeg. To explicitly choose a backend for a command:

```sh
AEDE_AUDIO_BACKEND=native aede play /path/to/song.flac
AEDE_AUDIO_BACKEND=ffplay aede play /path/to/song.flac
```

`native` requires that route rather than silently falling back; ffplay must be installed for its route. Aède sends processed `f32le` blocks to ffplay. The native path receives processed samples through a preallocated frame-aligned ring; its Aède callback does not allocate, lock or log. It can still have queue shortages or host errors. Aède records listens asynchronously so history writes do not block the decode loop.

## Read the stage report

Look for source/output sample rate and channel information, backend/format, normalization source and selected gain, the normalization headroom reserve, optional tone stage/reserve and rate conversion where active. A disabled stage differs from a stage unavailable because the format/path does not support it. The CLI reports that dynamic gain reduction is unavailable without a limiter; it does not invent a limiter meter from the hard guard.

## Output peak snapshot

The output meter observes complete guarded PCM submitted to the sink. It reports submitted frames, sample peak before/after the guard, an oversampled true-peak estimate when available, and the number of samples changed by the guard. A sample peak of 1 corresponds to 0 dBFS; smaller amplitudes are below full scale. Unknown/unavailable true peak must not be read as zero.

At 192 kHz and above, true peak is unknown because the current meter no longer oversamples. Meter failure can also make it unavailable. Incomplete output-span statistics are explicitly unavailable rather than claimed complete. Measurements precede native dither/device conversion and do not measure physical sound. [Source loudness](measurements.md) is measured separately before processing.

## Shortages, host errors and drain

Consumed-frame counters, bounded queue occupancy, queue shortages and host xruns distinguish late decoded data from driver scheduling/output problems. An **underrun** means the output needs samples before they arrive; exact silence may be emitted. Not every recoverable route/scheduling notification is a fatal error, but it remains visible as a warning.

Final drain and output-format changes keep transport controls usable. Native drain treats five seconds without consumed-frame progress as an error. The approximately 100 ms host allowance is counted in active time, not while paused, and is not a physical playback acknowledgement. These limits do not certify latency, seamless joins or NAS performance.

If playback fails, keep its diagnostic message, verify backend dependencies and file/layout compatibility, and compare with flat tone/normalization off. Do not delete original files or data stores to repair an audio-device error. [Continuity](continuity.md) separates tested PCM joins from unmeasured physical playback.
