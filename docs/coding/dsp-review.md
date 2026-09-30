# DSP implementation and playback review

Reviewed on 2026-09-30. This document records the current free DSP foundation, the corrections to slow normalization startup and per-track filter resets, and the remaining quality work. The [product proposal](../design/dsp-product-proposal.md) remains the feature roadmap; [playback design](../design/playback.md) describes the behavior of the player.

## Assessment

The architecture is suitable for a reliable music player: decoding, normalization policy, PCM processing and output transport are separate. The signal path uses established algorithms and existing Rust libraries rather than an unvalidated replacement for loudness analysis, equalization or resampling. This is a sound foundation, but implementation and passing software tests alone do not establish hardware gaplessness, absolute meter compliance, or a measured quality/performance budget on a NAS.

The DSP is shared code for local playback and a future server audio path. Only local playback currently uses it end to end; the server has no audio delivery route. Premium effects remain proposed work. A true-peak limiter, compression, convolution and parametric EQ are not supplied by the current free stages.

## Free functionality: implementation and actual limits

| Item | Current implementation | Remaining limits and evidence to obtain |
| --- | --- | --- |
| F1: output continuity and negotiation | CPAL keeps a device stream for consecutive compatible output formats; ffplay keeps its process. Formats and channel counts are explicit. | Source/device format changes can reopen output. Native selection ranks sample format before rate distance: a floating configuration at a different rate may win over an integer configuration supporting the source rate. Make this product policy explicit and test competing choices. |
| F2: gapless playback | Progressive decoding, encoder trimming for declared MP3 and Opus data, exact PCM joins in fixtures, bounded output queue and transport controls. Compatible natural joins share EQ and conversion state, with bit-identical whole-versus-split signal tests. | Native Vorbis ending without ffmpeg remains uncertain. Hardware continuity and underrun behavior are unmeasured. Opening/decoding a next file can still exhaust the bounded output buffer. |
| F3: metadata normalization | ReplayGain and Opus R128 gains, track/album/off policy, -18 LUFS playback target, explicit gain origin, Opus header gain handling. | The target is a playback choice, not EBU R128's -23 LUFS broadcast target. Metadata may be missing or inaccurate; headroom attenuation may prevent reaching the target. |
| F4: headroom and clipping policy | Selected normalization gain is capped using a known peak or a conservative full-scale assumption; positive tone boosts reserve preamp headroom. A final sample guard reports hard-clamped samples. | Static reserves do not guarantee an inter-sample ceiling or prevent every filter transient. The guard can cause distortion when it intervenes; it is not a transparent limiter. |
| F5: measured loudness | Current FlacCompagnon track analyses and persisted measurements are reused. `ebur128` supplies gated programme measurement, including multiple format histories. Missing measurements are learned from source PCM during complete playback. | Source loudness currently accepts mono/stereo only. Format changes start separate meter histories, so gating windows do not span that boundary. No true-peak estimate is claimed for a programme containing a rate at or above 192 kHz. An unknown track/album cannot be exactly normalized on its first play without prior analysis. |
| F6: basic tone controls | Optional bass/treble shelves, per-channel floating filter state, exact flat bypass, bounded settings and conservative boost preamp. Coefficients follow the Audio EQ Cookbook through `biquad`. State persists across blocks and compatible naturally advancing tracks. | Parameter changes need a deliberate transition policy before live editing is offered. Absolute frequency-response and transient checks remain useful. |
| F7: channel handling | Known speaker masks are retained. Multichannel sources can be downmixed to stereo with explicit coefficients, LFE omission and a conservative peak bound; unknown layouts are refused. | This is a documented downmix policy, not a guarantee of perceptual equivalence to a commercial decoder or the original multichannel mix. Wider layout and source-format fixtures remain useful. |
| F8: rate conversion | Rubato performs bandlimited fixed-ratio conversion when the chosen device rate differs. Ordinary rate pairs use FFT conversion; unusual ratios use sinc interpolation. The shared session retains converter state and cumulative frame rounding across compatible files, trims startup once and drains once at group end. Both paths have whole-versus-split comparisons. | The existing single-tone alias test is a starting point; rate sweeps and quantified CPU/memory budgets remain. Incompatible source formats require a fresh processing group. |
| F9: integer conversion and dither | Floating native formats are preferred. Supported integer output is quantized with continuous TPDF dither; floating output bypasses it and underruns produce exact silence. | The decoded `f32` path does not establish bit-perfect high-resolution integer output. ffplay's final conversion is outside Aède's control. The real-time properties of the surrounding native callback still need improvement. |
| F10: metering and diagnostics | Submitted sample peak, pre-guard peak, available oversampled true-peak estimate, guard count, actual stage status and static headroom are reported. Spectrum display is optional. | Measurements precede dither/device conversion and do not measure analogue output. Dynamic gain reduction is unavailable without a limiter. Absolute reference signals and independent comparisons are still required for a compliance claim. |

The code and sibling test files in [`aede-dsp`](../../crates/aede-dsp/README.md), [`aede-core::playback`](../../crates/aede-core/src/playback.rs) and the [CLI player](../../crates/aede-cli/src/commands/play.rs) provide the implementation evidence. The verification record below states which checks were actually run for this review.

## Slow startup: cause and corrected policy

The previous `plan_normalization` path loaded information for the entire selection and fully decoded each file or album without a reusable measurement before returning. An artist selection could therefore wait for a large part of a library to be analyzed before the first sample reached the output. Caching improved later invocations but did not make the first invocation usable.

This delay is not solved merely by changing the meter library. Exact integrated loudness depends on the complete programme and its gating decisions. Momentary or short-term readings of the beginning cannot be substituted without changing the meaning of normalization. [EBU loudness overview](https://tech.ebu.ch/loudness) and [R128](https://tech.ebu.ch/docs/r/r128.pdf) describe the distinction.

The corrected playback path uses [`ReadyNormalization`](../../crates/aede-core/src/playback/gain_plan_ready.rs):

- Prepare only the current track, or the current album's identities and tags. Choose a known exact-scope metadata gain, an applicable imported track result, or a valid cached measurement without fully decoding a selection before playback.
- Observe the source PCM already decoded for playback, before downmix, rate conversion, gain, EQ and output protection. This avoids a second audio decode.
- Keep a chosen gain fixed during playback. When album metadata is incomplete and no valid programme cache exists, retain an unadjusted, uniform album decision for that session and learn a complete programme for a later play. Per-track LUFS are not averaged into an album value.
- Publish a measurement only after full decoding of the track or complete ordered programme. Skips, stops, interrupted sequences and unexpected measurement errors do not publish a partial value as integrated track/album loudness. A complete silent or unsupported-layout source caches unavailability, not a partial measurement. Returning to an unknown album's first track can restart capture after interruption, with the session gain still frozen.
- Save completed updates outside the audio decoding loop, merge into the latest conclusions store under its writer lock, and check source identity again before publication. Measurement work does not create an additional listening-history event.
- Signal unavailable normalization and optional measurement failures to the user. A missing measurement remains a distinct state from disabled normalization.

This still has costs: current-album metadata inspection and loading the conclusions store are not free, and loudness capture adds CPU work during decoding. Source identity uses the project's size/mtime identity rather than a fresh cryptographic content hash. Benchmark startup and capture overhead on representative collections and target NAS hardware before promising a latency bound.

No DSP library was changed for this correction. The complete offline measurement helpers remain useful for an explicit preparation operation; ordinary playback no longer depends on completing them.

The derived loudness method version changes from 1 to 2. Earlier Aède measurements could label a non-oversampled peak at 192 kHz or above as a true-peak estimate. Version 1 playback caches are therefore expired and rebuilt from complete playback without blocking startup. Imported FlacCompagnon analyses are retained; this invalidates derived Aède measurements rather than deleting source analyses.

The pinned FlacCompagnon v0.9.5 source uses its own four-phase FIR peak estimator at every rate, not `ebur128`'s rate-dependent path. Its imported high-rate true-peak estimates therefore remain usable; their absolute conformance still needs reference-signal validation. See its [peak estimator](https://github.com/craft-and-code/FlacCompagnon/blob/8891e8326b2b38bd042c22f9f6ec617e1b2859e9/core/src/analysis/truepeak.rs) and [analyzer integration](https://github.com/craft-and-code/FlacCompagnon/blob/8891e8326b2b38bd042c22f9f6ec617e1b2859e9/core/src/analysis/analyzer.rs).

## Why this policy matches established implementations

The following sources support the chosen separation of analysis and playback; their behavior is a reference, not proof that Aède has the same measured performance.

| Source | Relevant behavior | Application to Aède |
| --- | --- | --- |
| [mpv ReplayGain options](https://mpv.io/manual/stable/#options-replaygain), [gain-selection code](https://github.com/mpv-player/mpv/blob/master/player/audio.c) | Playback applies stored gain, has an explicit missing-metadata fallback and can reduce gain using a peak. | Read known values at startup and expose the fallback; keep applying gain inexpensive. |
| [beets ReplayGain plugin](https://docs.beets.io/en/stable/plugins/replaygain.html), [source](https://github.com/beetbox/beets/blob/master/beetsplug/replaygain.py) | Analysis can run at import or by a separate command, skip existing values, use configurable concurrency and store results in its database. | Add explicit preparation with progress, cancellation and bounded concurrency if users need first-play normalization across a library. Retain Aède's prohibition on modifying tags. |
| [GStreamer rganalysis](https://origin.gstreamer.freedesktop.org/documentation/replaygain/rganalysis.html) | Track results are finalized at end of stream; album results are known after the last track and apply one gain to the album. | Capture during decoding, publish at completion, preserve album-relative levels. |
| [ebur128 API](https://docs.rs/ebur128/0.1.10/ebur128/struct.EbuR128.html) | Integrated results can combine meter histories. Parallel chunks require appropriate overlap and filter seeding; true-peak interpolation depends on rate and library version. | Keep programme energies/gating, avoid averaging LUFS, and identify the algorithm in persisted derived results. |

An adaptive normalization effect would be a separate product choice. [FFmpeg loudnorm](https://ffmpeg.org/ffmpeg-filters.html#loudnorm) distinguishes linear scaling with prior measurements from dynamic processing. Introducing dynamic gain changes to remove startup delay would change listening behavior and could alter musical dynamics.

## Continuous processing: corrected ownership and boundaries

The shared [`PcmSession`](../../crates/aede-core/src/playback/session.rs) now owns persistent `aede-dsp` tone and compatible conversion state. The CLI feeds decoded/downmixed source blocks into this session instead of recreating both processors for every file. Natural source EOF seals an attribution boundary without flushing the converter. Startup trimming and tail draining happen once per compatible group. Cumulative source-to-output frame rounding avoids an extra output frame at each file boundary.

Delayed output spans retain their source token, fixed gain, statistics and completion marker, even when the next file is already being decoded. Gain changes occur after conversion without resetting tone filters. These spans keep listening identities, durations and deferred loudness-cache publication attached to the correct track. Partial writes count only complete submitted frames, with unavailable aggregate diagnostics explicitly reported for an interrupted span. Next, Previous, Stop and errors during a track discard processing state and queued output; incompatible formats start a fresh group. Previous also works during the final output drain. The [playback design](../design/playback.md#continuous-processing-and-track-attribution) records the full lifecycle.

The earlier synthetic diagnostic exposed the old per-track discrepancy: two 10,001-frame constant signals at amplitude 0.25, converted from 44.1 to 48 kHz separately, produced 21,772 frames rather than the 21,771 frames of one continuous conversion. At the join, separately converted values were approximately 0.09945 and 0.24423, while the continuous result remained approximately 0.25. Resetting a +6 dB bass shelf gave approximately 0.25105 instead of the continuous 0.49882. This is the diagnosed baseline, not the behavior of the corrected session. Whole-versus-split comparisons now check exact samples and frame counts with EQ plus FFT or sinc conversion. These software comparisons do not establish physical gaplessness or perceptual audibility.

Rubato's streaming model and explicit delay/tail handling support this direction. Its maintainer advises preallocated processing buffers and avoiding repeated expensive construction. [Rubato documentation](https://github.com/HEnquist/rubato#real-time-considerations) provides the implementation guidance. Reuse a continuous filter for an album stream; resetting between independent offline clips is a different case.

[libsamplerate's FAQ, question 5](https://libsndfile.github.io/libsamplerate/faq.html) explains the same state requirement for chunked conversion: independent conversion calls cannot replace a stateful streaming converter. This supports the boundary tests; it does not require replacing Rubato.

## Remaining work in priority order

### 1. Strengthen the native callback's real-time contract

The current bounded standard-library channel and block handoff limit memory use and keep heavy processing off the callback. They do not establish a wait-free audio path: channel internals and destroying replaced sample vectors may still involve locks or deallocation. Device-error logging also belongs outside a time-critical path. Preserve bounded queues, pause/skip responsiveness and consumed-frame counters when changing this transport.

The native producer also converts `f32le` bytes back into an allocated sample vector for each write attempt, including an attempt rejected by a full queue. A direct native `f32` sink and retained/preallocated pending data can remove these repeated copies and allocations. Preserve `f32le` serialization for transports that actually need bytes.

PortAudio's [callback guidance](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html) recommends avoiding allocation/deallocation, I/O, mutex operations and other unbounded OS work in a callback. A preallocated SPSC ring buffer is a candidate; [`rtrb`](https://docs.rs/rtrb/latest/rtrb/) documents fixed capacity, wait-free reads/writes and no allocation after construction for plain sample elements. It is a future option, not a dependency added by this review. Record underruns and queue occupancy outside the callback to make hardware tests actionable.

### 2. Add independent absolute signal validation and performance budgets

Use the [EBU Tech 3341 specification](https://tech.ebu.ch/publications/tech3341) and [official loudness test set](https://tech.ebu.ch/publications/ebu_loudness_test_set) to test absolute LUFS/gating and true-peak readings, not only agreement between two block sizes. Include silence, short streams, mono/stereo, gain-scaled material and sample rates with and without oversampled true-peak estimation. A comparison to another BS.1770 implementation is useful additional evidence, but is not a substitute for expected values from reference signals.

For F8, measure passband ripple and alias rejection across rate pairs, near the new Nyquist limit and through a swept input, alongside delay, tails and block boundaries. Cover both FFT and uncommon-ratio sinc paths. Keep expected quality thresholds distinct from CPU/memory measurements. [CamillaDSP resampling guidance](https://github.com/HEnquist/camilladsp#resampling) is a concrete reference for selectable quality/CPU tradeoffs, and demonstrates use of Rubato in a larger Rust DSP engine. Its reported profile quality is not a result measured for Aède's settings.

Benchmark cold and warm startup, decode throughput, source loudness capture, output true-peak metering, EQ and resampling independently. Record release build, platform, input format/rate/channel count, queue margin and memory use. Set budgets using the intended NAS/device rather than an arbitrary development-machine result.

### 3. Close decoder and physical gapless gaps

Validate native Vorbis granule trimming with fixtures or explicitly retain ffmpeg as the exact-ending path. Record a loopback/device capture of consecutive test tracks, including format changes and output under CPU/I/O pressure. Check that output underruns are counted and visible. Validate physical sample continuity with the corrected continuous EQ/resampling session.

### 4. Decide rate-versus-format policy and future preparation workflow

Specify whether preserving a supported source rate is preferred over floating-point output at a different rate. Current format-first selection can cause conversion that a rate-first policy would avoid; either choice needs explicit behavior and tests. This is independent of operating-system mixer conversion and does not establish a bit-perfect device path.

If an explicit loudness preparation command or server task is added, provide progress, cancellation, invalidation and a bounded worker budget. Share it with the same `aede-core` measurement/cache rules. Automatic second-pass background decoding is not part of this correction; measuring the playback decode is already sufficient to learn completed unknown material without duplicate file reads.

## Primary signal references and reusable code

- [ITU-R BS.1770-5](https://www.itu.int/rec/R-REC-BS.1770-5-202311-I): programme loudness and true-peak algorithms.
- [EBU Tech 3341](https://tech.ebu.ch/publications/tech3341) and [test signals](https://tech.ebu.ch/publications/ebu_loudness_test_set): meter definitions and minimum requirement test material.
- [W3C Audio EQ Cookbook](https://www.w3.org/TR/audio-eq-cookbook/): original biquad coefficient formulae adapted from Robert Bristow-Johnson, including shelf slope and normalization.
- [`ebur128` code](https://github.com/sdroege/ebur128) and [API](https://docs.rs/ebur128/0.1.10/ebur128/struct.EbuR128.html): existing Rust loudness implementation, rate-dependent true-peak estimation and aggregation.
- [Rubato code/documentation](https://github.com/HEnquist/rubato) and [CamillaDSP](https://github.com/HEnquist/camilladsp): existing resampling library and a larger Rust processing engine useful as architectural/measurement references.
- [PortAudio callback guidance](https://portaudio.com/docs/v19-doxydocs/writing_a_callback.html) and [`rtrb`](https://docs.rs/rtrb/latest/rtrb/): real-time constraints and a possible preallocated transport primitive.

These references support retaining the current established libraries. They do not justify wholesale importing another engine, adding effects without a use case, or promising measured quality that has not yet been checked on Aède.

## Validation record

The normalization-startup correction's `tools/check.sh` run on 2026-09-30 passed: build-helper tests, formatting, warning-free lint, 1,137 Rust tests including the doctest, warning-free documentation and release build. There were no ignored Rust tests. Local socket tests were run with the required sandbox permission. The first run identified the missing README entry for this review; that documentation omission was corrected before the successful runs.

The regression coverage includes 18 lazy-normalization tests (import/tag/cache priority, stale analyses, complete-source publication, skips, album restart, unavailable-layout caching and source changes), source-observer tests before downmix/resampling, an end-to-end CLI test proving a corrupt later file cannot prevent first-track PCM output/history, and high-rate programme tests. Existing tests were retained. The high-rate peak and album-restart/unavailable-layout regressions were reproduced as failing tests before their fixes.

### Continuous-session verification

The subsequent complete `tools/check.sh` run passed all the same gates with 1,150 Rust tests including the doctest and none ignored. Seven new core tests cover bit-identical whole/split processing with EQ and FFT/sinc conversion, fixed per-track gains, fractional cumulative boundaries, short/empty completions, delayed attribution, lifecycle errors and real WAV decoding with raw source observation. Three CLI integration tests cover continuous EQ across files, format-change resets and a corrupt later file preserving previous PCM/history. Three driver unit tests cover partial output acceptance, interrupted-write retry and delayed short-track history. Existing playback tests retain their behavior and count. The CLI boundary, partial-acceptance and interrupted-write regressions were reproduced as failures before correction.

An isolated terminal test held a fake ffplay consumer open after EOF, sent Previous during final drain, then Stop during the restarted drain. Both 38,400-byte outputs with +6 dB bass and -3 dB treble were identical, confirming a fresh processor on restart; the command exited successfully and saved two incomplete listens and a count of two. It used synthetic WAV data and a separate temporary Aède data directory, with no real audio output. The report is `/private/tmp/aede-dsp-pty-9xc45exr/report.json`; the reusable driver is `/private/tmp/aede-dsp-pty.py`. These transport checks do not measure hardware playback.

### Synthetic startup comparison

The preserved earlier release and final rebuilt release were run on this macOS development host against the same 24 synthetic stereo 48 kHz PCM16 WAV tracks of 30 seconds each: 12 minutes of source audio. Every run used a new Aède data directory with no measurements. A fake ffplay consumer received bytes without producing sound or waiting for real-time playback; the parent detected its first-PCM marker at 1 ms polling intervals, with a common wall clock as a cross-check. Source files were already present; this is not a cold-disk test.

| Release and mode | First submitted PCM | Complete fast consumption | Received bytes |
| --- | ---: | ---: | ---: |
| Earlier release, track normalization | 1.4355 s | 2.9762 s | 276,480,000 |
| Earlier release, normalization off | 0.0419 s | 1.4246 s | 276,480,000 |
| Final release, track normalization with playback capture | 0.0420 s | 2.5597 s | 276,480,000 |

All runs exited successfully and received exactly the expected frame count. The final run no longer reported preflight loudness preparation. The unknown source intentionally retains its decoded level on this first run; the earlier release had premeasured normalization. These byte counts demonstrate complete delivery, not identical sample values across the different gain decisions. The approximately 34-fold reduction concerns startup in this synthetic scenario; it is not a measured audible-device latency or a promise for a real Ozzy selection or NAS. Total capture CPU work remains visible in fast consumption, so target-device performance budgets are still needed.

Raw temporary reports are `/private/tmp/aede-dsp-bench/before-0d5b7969/summary.json`, `/private/tmp/aede-dsp-bench/before-off-adeaca30/summary.json` and `/private/tmp/aede-dsp-bench/after-final-16c6e844/summary.json`; the isolated driver is `/private/tmp/aede-dsp-bench/bench.py`. They use no user music or Aède data. Earlier binary SHA-256: `228fd6d289181f9a1fd365103e0c74c5cb723ab9dd465218c365c941a238cb76`; final binary SHA-256: `0c376704ce9f6243c26524d292e71356b276270d6bd7345dbba4dbb3b40c38ac`.

No official EBU signal-set validation, physical loopback, target-NAS benchmark or blinded listening test has been performed in this review. The historical join diagnostics exposed software state discontinuities now covered by regression tests; they do not establish perceptual audibility.
