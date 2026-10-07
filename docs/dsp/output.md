# Output stages, meters and diagnostics

The sound you hear includes Aède's processing, the output backend, the operating system and the device. Diagnostics identify the part Aède can observe rather than calling the whole path transparent or bit-perfect.

## Native output and fallback

Ordinary `aede play` uses native CPAL when a compatible channel/sample format is available, otherwise ffplay. Without-effects is the local default and prefers the source rate; DSP keeps floating-format preference. Strict bit-perfect uses the separate direct ALSA backend on glibc Linux and requires an explicit eligible hardware device; other current platform routes are refused. Opus and M4A can require FFmpeg decoding fallback. To explicitly choose an ordinary backend for a command:

```sh
AEDE_AUDIO_BACKEND=native aede play /path/to/song.flac
AEDE_AUDIO_BACKEND=ffplay aede play /path/to/song.flac
```

`native` requires that route rather than silently falling back; ffplay must be installed for its route. Aède sends processed `f32le` blocks to ffplay. The native path receives processed samples through a preallocated frame-aligned ring; its Aède callback does not allocate, lock or log. It can still have queue shortages or host errors. Aède records listens asynchronously so history writes do not block the decode loop.

`aede play --list-devices` lists native names and IDs without opening playback. An explicit `--output-device` disables automatic substitution/fallback; strict mode also refuses forced ffplay. See [playback policies](../cli/play.md#playback-policies-and-strict-output) for admission and static Linux archive limits. Neither a listed device nor successful software route admission proves physical digital identity.

Direct ALSA has software simulation and synthetic Linux-configuration API checks; native Linux compilation, linking, runtime and digital-capture acceptance remain unverified. Those checks must succeed on the target platform before its hardware output can be advertised as validated.

## Read the stage report

Look for the playback policy, device/backend and format, source/output rate and channels, normalization source and gain, normalization/tone reserves, and any required conversion. Without-effects reports adaptations and any precision-reducing integer conversion rather than claiming strict output. Strict reports source preservation separately from unmeasured physical capture. A disabled stage differs from an unavailable one. Dynamic gain reduction remains unavailable without a limiter; the CLI does not invent a limiter meter from the hard guard.

## Output peak snapshot

The output meter observes complete guarded PCM submitted to the sink. It reports submitted frames, sample peak before/after the guard, an oversampled true-peak estimate when available, and the number of samples changed by the guard. A sample peak of 1 corresponds to 0 dBFS; smaller amplitudes are below full scale. Unknown/unavailable true peak must not be read as zero.

At 192 kHz and above, true peak is unknown because the current meter no longer oversamples. Meter failure can also make it unavailable. Incomplete output-span statistics are explicitly unavailable rather than claimed complete. Measurements precede native dither/device conversion and do not measure physical sound. [Source loudness](measurements.md) is measured separately before processing.

## Shortages, host errors and drain

Consumed-frame counters, bounded queue occupancy, queue shortages and host xruns distinguish late decoded data from driver scheduling/output problems. An **underrun** means the output needs samples before they arrive; exact silence may be emitted. Not every recoverable route/scheduling notification is a fatal error, but it remains visible as a warning.

Strict output treats programme underruns, route changes and unknown output errors as terminal preservation failures, with no automatic recovery. Its history counts native-consumed source frames and requires successful source EOF before completion. The ordinary submitted/active-time history and remote client acknowledgements remain separate evidence; none is a physical DAC acknowledgement.

Final drain and output-format changes keep transport controls usable. Native drain treats five seconds without consumed-frame progress as an error. Ordinary CPAL adds an approximately 100 ms host allowance in active time, not while paused; direct ALSA uses reported hardware delay and successful nonblocking drain instead. Neither is physical proof that a sample emerged from the DAC. These limits do not certify latency, seamless joins or NAS performance.

If playback fails, keep its diagnostic message, verify backend dependencies and file/layout compatibility, and compare with flat tone/normalization off. Do not delete original files or data stores to repair an audio-device error. [Continuity](continuity.md) separates tested PCM joins from unmeasured physical playback.
