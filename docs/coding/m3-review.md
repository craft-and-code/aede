# M3 software hardening

Reviewed on 2026-10-04. This checkpoint covers the implemented player and all current DSP stages. It does not establish physical gapless playback, certification of meters or native Windows/Linux deployment acceptance. The [DSP review](dsp-review.md) retains signal-quality budgets and measurements; [playback design](../design/playback.md) and the [native wire contract](../server/playback.md) define behavior.

## Scope and corrections

| Area | Finding and resulting behavior |
| --- | --- |
| DSP numerical safety | Finite extreme PCM could panic inside Rubato's FFT. The converter now rejects unsafe magnitudes before changing state, supports ordinary above-full-scale PCM without silently clipping it, and refuses non-finite internal output. Recovery after input rejection is checked against a fresh stream. Tone-processing overflow remains an explicit failure: abandon the partially processed buffer/filter. |
| Converter work | A large push repeatedly moved the remaining input after each chunk. A cursor now consumes chunks and compacts only between pushes, retaining exact output counts, tails and samples. |
| Source loudness | Size and whole-second modification time missed same-size changes within a second. Derived track/album caches now carry optional nanosecond evidence; method v5 expires older derived measurements. Reuse, capture completion and publication recheck precise identity. Imported FlacCompagnon analyses retain their original provenance and whole-second evidence; no precision is invented for them. JSON format version is unchanged and the SQL mirror includes the optional fields. |
| Native normalization | Server playback never observes or publishes new source loudness. Its reuse-only policy now avoids preparing unused meters while preserving existing fallback/headroom policy. |
| Local decoding | A relative local filename starting with `file:` could be interpreted as a protocol by FFmpeg. Fallback input is now a canonical regular local file with input protocols restricted to `file,pipe`. Real Opus/AAC/ALAC fixtures and progressive seeking retain coverage. See the [FFmpeg protocol contract](https://ffmpeg.org/ffmpeg-protocols.html#Protocol-Options). |
| Persistent source keys | Playback selection, loudness identity and personal history refuse non-UTF-8 paths rather than aliasing different names through lossy conversion. |
| Lyrics identity | A pre-epoch or unavailable modification time could become a valid zero timestamp. Catalogued and directly read lyrics now refuse that ambiguous source evidence. |
| Remote sources and scheduling | Native PCM refuses relative/traversing catalog paths before opening. Catalog/source-analysis selection runs outside Tokio network threads under the existing playback admission limit. Unused interactive initialization outputs and redundant clones were removed. |
| Remote cancellation | Existing deadline tests now check that duplicate/invalid ACKs cannot extend progress time. A combined real-socket regression fills all four playback slots and ACK windows, then checks shutdown releases workers without inventing listening history. |
| Terminal spectrum | Producer-side analysis schedules bounded frame-indexed FFT snapshots; presentation waits for the shared consumed-frame clock. Compatible joins retain partial windows/peaks, while seeking, skipping and output resets discard them. Pause freezes presentation. No FFT, allocation or terminal work was added to the audio callback. |
| Terminal workload | Playlist reading is capped at 16 MiB before/during loading; Unix key input has a bounded 64-action backlog. Selected labels are indexed once instead of rescanning the catalog on repeats. Width queries run once per second instead of four times per second. |
| Seek history | Submillisecond submitted segments now accumulate before final millisecond rounding. Eight one-frame segments at 8 kHz retain one millisecond in one incomplete listening record rather than disappearing. |

No new dependency or unsafe code was introduced. There was no confirmed dead production module warranting deletion. The implementation audit removed the unused remote outputs/clones above without changing the public wire contracts or rewriting audio/tags.

## Performance and presentation evidence

The matched release microbenchmark used ten seconds of 48 kHz stereo, three trials, and 1024-frame versus whole 480000-frame pushes. All 54 frame-count/checksum results match before/after. Median FFT 48→44.1 kHz processing/finalization for a whole push fell from 26.862 to 10.715 ms (2.51×); unusual sinc 48000→48001 Hz fell from 83.112 to 69.076 ms (1.20×). Small pushes have comparable cost. These measurements isolate DSP on the development host, excluding decoding, output and network; they are not a NAS budget. Reproduction and limitations are in the [DSP review](dsp-review.md#m3-hardening-numerical-safety-and-large-input-blocks).

At 48 kHz, a spectrum window spans 2048 frames (about 43 ms). Snapshot timestamp quantization adds at most 256 frames (about 5 ms), with a 50 ms terminal redraw interval. Native snapshots wait for callback consumption rather than showing the 500 ms audio buffer in advance. The pending snapshots are capped at 256; excessive visualization is discarded instead of waiting for capacity. Rendering still uses synchronous terminal I/O on the producer: a stalled terminal/SSH connection can exhaust audio lookahead. The device/mixer latency and terminal scheduling are unmeasured; ffplay uses the marked active-time estimate. This is not a physical audio synchronization guarantee.

## Test assessment and verification

Confirmed defects were first reproduced by failing regressions, including same-second cache changes, malformed source paths, numerical overflow, protocol-like filenames, lyrics timestamps, spectrum anticipation and lost tiny seek segments. Added tests cover observable refusal, retained state, exact output, persistence and shutdown; assertions were also added to existing roundtrip, ACK, label and position tests. Test-only code remains in declared sibling files. No behavioral test was removed or disabled merely to shorten the gate.

The DSP suite retains independent absolute loudness/peak expectations, analytic rate-conversion rejection/phase checks, encoder-trim fixtures, compatible whole/split processing and finite-tail tests. The slow unusual-ratio quality case is useful: it protects previously demonstrated bandwidth/alias defects rather than duplicating a construction assertion. It does not certify every loudness standard test signal or every possible conversion.

The default-feature gate passes with Rust 1.89: 1923 Rust tests, none ignored, including three doctests. The no-default-features workspace check passes 1906 tests with warning-free all-target Clippy and RustDoc; Rust 1.99 all-target Clippy also passes. Nineteen focused unit regressions were added to the previous implementation. The final executed verification checkpoint is recorded in [current state](current-state.md#test-status). The authoritative offline gate includes helper/site tests, formatting, warning-free all-target Clippy, required FFmpeg fixtures, workspace tests, real account pseudo-terminals, compiled active-test statistics, bilingual documentation, RustDoc/link checks and release compilation. Public statistics remain generated from authored sources, compiled test inventories and Git history, with stale fingerprints refused; no numeric claim is maintained in website source.

The separate live OSV audit covers 235 locked crates.io versions. It reports the existing [`paste` maintenance warning](https://rustsec.org/advisories/RUSTSEC-2024-0436.html); the pinned Git `flaccompagnon-core` dependency is outside registry-advisory coverage. Passing this audit does not prove the absence of vulnerabilities in all dependencies or external FFmpeg installations.

## Remaining acceptance and designs

- Physical local/remote gapless and idle/CPU/I/O-load captures using the [measurement procedure](gapless-measurement.md), once an actual return path is available.
- Native Windows console/device acceptance, Linux/Windows CI confirmation and target-NAS throughput/memory/service lifecycle. These need their respective environments.
- Full loudness reference/certification work and target-device DSP budgets. Current shelves, gain/headroom and sample guard are not a true-peak limiter; proposed premium DSP remains separate work.
- Local history still reflects submitted complete frames and active-time estimates, potentially preceding native consumption by its pending buffer. Precise consumed-frame local history is a future contract; native remote history separately follows client ACKs, which themselves do not prove acoustic output.
- Local CLI live queue editing/persistent sessions and additional named shuffle modes remain designs, as recorded in the [roadmap](../design/roadmap.md). M4 device protocols and Docker publication remain deferred.
