# M3 transport and playback safeguards

Local native/ffplay playback, progressive decoding, remote PCM/ACKs, queue occurrences, private checkpoints and DSP have separate output/consumption evidence. See [playback design](../design/playback.md), [DSP processing](dsp-review.md) and [strict acceptance](bit-perfect-acceptance.md).

## Retained regression contracts

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

## Scheduling and history

Device callbacks perform bounded sample transfer without filesystem/network operations, blocking storage or logging. Decode, DSP planning, FFT/metering and private checkpoint/history work stay outside the callback. Compatible continuous sessions preserve converter/gain state and exact occurrence order; rejected future-order edits are atomic. Repeated references remain separate occurrences. Source mismatch, MD5 errors and stale seek epochs refuse publication.

PCM credential/policy confirmation observes the captured shutdown receiver and has a two-second wait budget. The same guard applies before start, at format/audio/EOF publication, to acknowledgements and controls, and during interactive reset acknowledgement. Dropping a timed-out credential wait does not release its blocking-worker admission early or turn unacknowledged PCM into listening history. Producer, checkpoint and quota cleanup retain their separate completion and filesystem limits.

Pre-stream policy reads share the bounded account/policy read admission, retained by their real worker after cancellation. Successful policy or quota confirmation repeats the captured-login check before authorizing more PCM; reconnecting does not transfer a pending confirmation to the successor login. The publication marker must also remain unchanged through this final identity wait.

At 48 kHz a spectrum window spans 2,048 frames (about 43 ms); timestamp quantization adds at most 256 frames and the terminal redraw interval is 50 ms. Up to 256 pending visualization snapshots are retained; excessive visualization is dropped rather than blocking. Native visual timing follows callback consumption rather than the audio lookahead. Terminal rendering remains synchronous on the producer, so stalled terminal/SSH output can exhaust lookahead; physical latency remains unmeasured.

Local history reflects submitted complete frames and active-time estimates, potentially ahead of native consumption by its pending buffer. Remote history follows client ACKs, which still do not prove acoustic output. Tiny seek segments accumulate without losing their fractional time. [Gapless measurement](gapless-measurement.md) defines idle/load captures independently of these software contracts.

## Verification and remaining work

Behavioral fixtures cover source identity, malformed paths, complete/partial decode, source mismatch, repeated queue entries, numerical overflow, seek resets, EOF/drain and owner/session revocation. Test-only code lives in declared sibling files; no meaningful test is removed merely to reduce execution time. [DSP reference/performance evidence](dsp-review.md) isolates processing rather than target-NAS budgets. [Current state](current-state.md) and the full gate describe verified scope.

Physical local/remote continuity, actual Windows console/output, strict ALSA runtime/digital capture, platform load and target-device budgets still need their environments. Local CLI live queue editing/persistent sessions and additional named shuffle modes remain roadmap designs. Initial M4 protocols now exist; richer device operation and deployment remain separate acceptance work. DSP shelves/headroom/sample guards are not a true-peak limiter or full loudness certification.
