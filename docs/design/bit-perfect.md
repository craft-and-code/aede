# Transparent playback and bit-perfect contract

**Status:** the design contract, integer decoder, unmodified session, exact adapters and shared native transport are implemented. `aede play` exposes without-effects, bit-perfect strict and DSP policies; without-effects is the new local default. A separate direct ALSA implementation targets glibc Linux hardware routes. CPAL routes remain ineligible for strict playback. Native Linux compilation, linking and runtime acceptance of the direct backend, followed by physical digital-capture acceptance, remain unverified. Saved settings and the native PCM wire contract are unchanged.

[`IntegerFileDecoder`](../../crates/aede-core/src/playback/decoder/integer.rs) decodes admitted native FLAC and PCM WAV progressively into canonical signed `i32` containers, with the original 16/24 valid bits, sample rate and channel association carried by `IntegerPcmFormat`. It retains source-depth values rather than output-container scaling. FLAC shares the existing metadata, frame/error and MD5 validation with the float decoder; WAV validates bounded RIFF chunks and admits ordinary 16/18-byte PCM format headers or 40-byte extensible PCM with matching valid/container precision. Strict local playback selects this source API and the separate exact session described below.

## Purpose and terminology

Separate convenient playback without selected effects, strict preservation of the decoded signal, and intentional DSP. Keep the existing Symphonia/CPAL separation and specialized DSP libraries; the [Rodio decision](playback.md#decision-keep-direct-cpal-output-rather-than-introduce-rodio) explains that architecture choice.

For the first strict profile, the reference is the ordered, signed integer PCM decoded from the lossless source, together with its valid bit depth, sample rate and channel layout. Bit-perfect means preserving those sample values, their channel association and their timing at the digital output. It does not mean transmitting FLAC/container bytes to the DAC, preserving metadata bytes, or proving an analogue waveform identical to stored integers.

Distinguish three boundaries:

1. **Source decoding:** establish the reference PCM and validate the file. A present FLAC MD5 is verified only at complete EOF; a missing digest is not an identity test failure or proof of corruption.
2. **Aède output submission:** prove that every submitted source frame is represented exactly, with no processing, omissions, duplication or reordering.
3. **Digital device output:** validate the selected host/driver/device route with a digital capture. Submission equality alone cannot prove that a system mixer, driver or DAC applies no processing.

A file-integrity verdict, an output-rate display and an exclusive-mode request are not substitutes for these separate checks. Aède cannot promise protection of loudspeakers, listening volume or absence of reconstructed inter-sample peaks from sample preservation.

## Three playback policies

These are local CLI policies selected with `--playback`; they do not extend native PCM v1 or client APIs. See [the play guide](../cli/play.md#playback-policies-and-strict-output) for usage.

| Policy | Processing | Incompatible output | Selection |
| --- | --- | --- | --- |
| `without-effects` | No selected normalization, EQ, limiter or other effect; prefer the source rate and an exact representation. Required conversion/downmix and an explicit output guard may still apply and must be reported. | Adapt using the supported playback path, reporting each intervention. Never claim strict preservation merely because effects are off. | Local default. |
| `bit-perfect` | Preserve the reference PCM; bypass all modifying stages. Permit only exact representation changes defined below. | Refuse before submitting affected source audio. Never silently switch policy, resample, downmix, reduce precision or fall back to ffplay. | Explicit policy and output device; persistence per output is later work. |
| `dsp` | Apply explicitly selected normalization, filters and profiles, with conversion/headroom/output protection as required. | Use the configured supported route and report applied processing. | Explicit policy or explicit non-neutral effects. |

Without an explicit policy, non-neutral track/album normalization or bass/treble settings select DSP; neutral options retain without-effects. An explicit DSP policy keeps automatic album normalization for catalogued albums and track normalization otherwise unless the user overrides it. Outside DSP, normalization defaults to off. `--output-device` requires a named native route without default substitution or ffplay fallback; strict mode requires this option. `play --list-devices` lists names and host-qualified IDs without starting audio. No policy/device settings are saved implicitly.

Effects incompatible with strict mode are errors, not ignored settings. Neutral settings such as normalization off and flat tone are compatible. Activating DSP while strict playback is running must require an explicit policy transition and a fresh validated path; it cannot silently modify that stream. Source/output diagnostics must identify the actual policy and interventions.

## Initial strict profile

The first implementation targets **local native playback** only, using existing selection/queue behavior. It must not bypass file-validation, cancellation or listening-history rules.

| Surface | Initial scope |
| --- | --- |
| Native FLAC | One native FLAC stream with integer PCM, 16 or 24 valid bits, mono or stereo. The existing bounded metadata view and MD5/error handling remain applicable; a leading ID3 block accepted by the existing reader does not change the audio reference. |
| WAV | Little-endian RIFF/WAVE integer PCM, 16 or 24 valid bits. Accept ordinary PCM and WAVE_FORMAT_EXTENSIBLE only when the PCM subtype, valid bits, container size, block alignment and mono/stereo layout are validated. This baseline uses matching valid/container bit depths; 24 valid bits stored in a 32-bit source container is a later extension. |
| Channels | Mono stays mono; stereo stays left/right in order. No mono duplication, channel mixing, LFE policy or surround fold-down. Unknown/conflicting speaker declarations are refused. |
| Rate | Keep the source rate exactly. A rate is eligible only when the selected native route supports it and can open it without rate conversion. The acceptance grid includes 44.1, 48, 88.2, 96, 176.4 and 192 kHz where the tested device supports them; unsupported rates are explicit refusals, not promised capabilities. |
| Output | Same-depth PCM or a proven exact representation with sufficient valid precision. The negotiated container width alone does not establish effective precision or downstream transparency. |
| Selection | Files, existing folders/playlists/catalog selections and their intentional duplicates retain their current ordering rules. Check eligibility when preparing each occurrence; do not decode an entire library or selection before starting. |

Out of scope for the first strict profile: Ogg FLAC, compressed WAV, RF64/RIFX, other integer source depths, floating-point source files, MP3/AAC/Opus/Vorbis, DSD/DoP, multichannel passthrough, FFmpeg/ffplay fallback, browser output, remote native PCM and network-player certification. Sources outside this profile remain available through ordinary supported playback; strict mode refuses them explicitly.

Determine eligibility from bounded parsing and decoding of the same opened regular file, not its extension or cached catalog properties. Require complete frames and consistent source declarations; truncated data and decoder failures are terminal errors. For WAV extensible, admit canonical mono or front-left/front-right stereo only, rather than erasing an unusual mask by inferring layout from channel count. The dedicated integer WAV adapter provides this admission separately from FlacCompagnon's unchanged float preview path. It requires an exact RIFF/file extent, one format chunk and one complete-frame data chunk; duplicate declarations, missing padding and waveform lists are refused. Extensible mono accepts an unspecified or center mask as a single mono channel, while stereo requires explicit front-left/front-right.

This source scope does not imply that every platform offers a suitable output. The first direct route is scoped to glibc Linux ALSA and admits only runtime-established route facts. The explicit ALSA dependency and narrowly scoped vendored readback patch are separately authorized implementation choices; they do not certify hardware or authorize additional native integrations. Extend platform and digital-capture acceptance independently.

## Exact representation and forbidden processing

For a signed source sample `s` with `b` valid bits, its normalized reference value is `s / 2^(b - 1)`. This definition includes the negative full-scale value and the asymmetric positive endpoint.

- Keeping the same signed representation is exact. Decode/output adapters must establish whether their library has already scaled or aligned a sample; never apply widening twice.
- Expanding to a wider signed PCM representation is exact only if the normalized value stays unchanged. For full-width PCM of `B` bits, that means `s * 2^(B - b)`, not unscaled sign extension. For example, 16-bit `1` becomes full-width 24-bit `256`; packed 24-bit bytes and a 24-bit sample held in a 32-bit container require the adapter's documented alignment rules. Compare canonical sample values, not unequal container bytes.
- Narrowing valid precision, truncating low bits, rounding with loss or adding dither is forbidden. Padding bits are not additional source resolution.
- Preserve integer samples and valid-bit metadata in the core path. A floating-point-only device interface is not automatically disqualified: correctly normalized 16/24-bit integers can be represented exactly in `f32`. Such an adapter qualifies only with exhaustive or equivalent conversion evidence and separately validated downstream precision; a generic float-first or float-roundtrip assumption is insufficient. Floating-point source support remains outside this first profile.
- Byte order, packing and interleaving changes are allowed only when invertible and channel/frame identity is unchanged.
- Gain, ReplayGain/R128, EQ, resampling, downmix, balance, crossfade, convolution, limiter and dither are bypassed. The hard output guard must not silently modify a strict sample; invalid source/adapter values cause an error.
- Integrity checks and analysis may observe a separate view without changing playback buffers. Meter failure must never enable processing or label unknown output evidence as verified.

For example, a compatible FLAC 24-bit/96-kHz stereo source may use an exact 32-bit output container at 96 kHz. A 16-bit-only output, 48-kHz-only output or mono-only output must be refused. The same refusals apply even if that conversion would be subjectively inaudible.

## Implemented software bypass and exact adapters

[`ExactPcmSession`](../../crates/aede-core/src/playback/exact_session.rs) accepts the typed decoder's canonical integers in blocks of at most 4096 complete frames. It has a fixed source and output format and no normalization, gain, tone, resampling, downmix, guard, limiter or dither controls. It preserves source samples in order and encodes the same frames through [`ExactPcmAdapter`](../../crates/aede-core/src/playback/exact_output.rs). Invalid frames, source-depth values, oversized blocks and lifecycle operations are refused before replacing output or advancing frame counts. Buffers are reserved before use; this work stays outside the device callback.

The adapter supports little-endian full-width signed 16-bit, packed signed 24-bit and full-width signed 32-bit PCM, plus normalized IEEE binary32/binary64. Signed widening left-aligns once to preserve the normalized value; it never merely sign-extends into a wider full-width output. Binary32/binary64 mapping is checked exhaustively for every signed 16-bit and 24-bit source value. Output metadata separates representation width from caller-known downstream precision. Unknown or insufficient precision, a different rate or a different layout is refused. These caller-supplied facts are software admission inputs, not evidence that a sound device or operating-system route actually has those properties.

Exact sessions reuse the processed session's `TrackSpan` occurrence attribution. Every accepted block has one source span and an observational normalized sample peak, with zero guard interventions. This independent meter view does not change source PCM. Successful decoder EOF seals one occurrence with an empty completion span; natural compatible joins insert no samples or delayed tail. Empty sources complete once. Completion marks production, not submission, consumption or listening history. The driver must seal only after successful EOF, including FLAC MD5 verification, and retain its existing acceptance/history rules. Seeking, cancellation and output errors discard the interrupted session and queued audio. Compatibility includes source valid precision, rate/layout and complete output metadata; a change needs a fresh session even if a later native sink can reuse its device stream.

The existing CLI bounded ring is generic over guarded `f32` or prevalidated integer PCM, without introducing another queue. Strict queue mode latches a missing source span or a callback ending inside a frame as failure. Further writes fail and subsequent callbacks emit only silence without consuming remaining source. Host errors can latch the same queue failure through a callback-safe atomic handle. Startup before source submission and genuinely completed source tails are distinct from programme underruns; producer publication/closure races are tested.

## Native driver integration and current admission limits

The same local transport driver now supports processed and exact source/session variants. The exact variant retains typed source integers through the native queue, bypasses loudness preparation/measurement/cache publication, and rejects non-neutral normalization or tone settings before opening a source. Observational meters and visualizers receive a separate exact normalized view. The internal four-byte handoff representation is used for existing frame accounting; it is not a claim about the device's byte packing.

Exact history follows callback-consumed source frames rather than decode lead or wall-clock time. Successful source EOF, including a present FLAC MD5 verdict, and actual queue consumption are both required for occurrence completion; the last occurrence also waits for the existing output-tail drain. Seeking retains an incomplete visit, counts only consumed segments, and samples counters before abort/reset. A later unsupported source or refused route drains valid preceding audio and preserves its completed records. Compatible repeated occurrences retain one stream; bounded lookahead starts a prefilled stream when necessary without inventing a format restart.

Native selection uses a host-qualified device ID exposed through `--output-device`. A missing named device does not select the default or fall back to ffplay. Strict negotiation keeps the source rate and channel layout first, then chooses sufficient effective precision and a proven exact representation. Unknown route facts are refusals; neither a container width nor a user assertion supplies missing hardware evidence. Without-effects now prefers source-rate eligibility, while DSP retains its processing policy. Strict route changes, host xruns, scheduling failures and ambiguous host errors latch failure and prevent further publication or automatic recovery.

The exact native integration reserves its queue before use and requests startup only when it is filled, explicitly started for bounded lookahead, or closed after a verified short programme. Pause before startup retains its intent. CPAL's ALSA worker can run callbacks after stream construction, so strict ALSA uses a separate direct backend with its own startup/lifecycle rather than treating `Stream::play` as evidence. The exact software adapters support signed 16-bit, packed 24-bit, signed 32-bit and exact normalized float mappings; the direct backend admits only representations it can establish. There is no strict quantizer, dither or clamp. These mappings and lifecycle rules have software tests, not physical output acceptance.

**No CPAL route is admitted as strict.** The pinned CPAL 0.18.1 safe API is insufficient for the required observations; direct ALSA is a separate implementation:

| Backend | Reason for refusal |
| --- | --- |
| WASAPI | CPAL opens shared mode with automatic PCM conversion; it does not expose an exclusive stream. |
| CoreAudio | The physical-format request is best effort and permits AudioUnit conversion. The API does not establish exclusive access, physical effective precision, channel mapping and unity system gain. |
| ALSA `hw:` | The hardware name is a candidate, not evidence. CPAL does not expose the opened PCM handle's effective precision, negotiated mapping and relevant controls. |
| ALSA plugins or other backends | Default, plug, mixer and unknown routes have no verified transparent profile. |

The first direct backend reuses the already-transitive ALSA library through an explicit target-specific dependency, with a narrow vendored patch exposing the missing native readback operations. It accepts explicit hardware routes rather than ALSA plugin/default/mixer routes. The opened PCM must provide the exact source rate, sufficient effective precision, compatible channel mapping and neutral supported controls; unknown or conflicting facts are refused. Linux static-musl builds still use ffplay and do not compile this direct backend. Subsequent macOS/Windows support requires its own direct/exclusive implementation and validation. None of these refusals proves that hardware is incapable of transparent output; they state what this implementation can establish.

The public policies are local CLI behavior. Ordinary fallback remains available unless a device/native backend is explicitly required; strict mode never falls back. Persisted settings and native PCM v1 stay unchanged. Digital capture and platform/device acceptance remain step 6. Every supported policy retains the source FLAC MD5 check during decoding; this is not a continuous digest of the sink or proof of downstream sample identity.

### Direct ALSA implementation boundary

Pure worker/policy simulations exercise software behavior on the development host. A metadata-only Rust harness with synthetic Linux GNU configuration checks gated API signatures; it does not use a native Linux standard library, linker, ALSA driver or device. Neither check establishes native Linux compilation/linking/runtime acceptance or physical sample identity. Keep those platform checks and the subsequent digital-capture matrix explicitly pending.

The [pinned ALSA-rs patch](../../vendor/alsa-0.11.0/AEDE-PATCH.md) adds three safe, read-only getters within the dependency's existing owned-handle/FFI boundary: opened PCM hardware type, effective significant bits and the configured rate numerator/denominator. It preserves native errors, rejects a zero denominator and exposes no raw pointer. The package remains ALSA-rs 0.11.0, with its original licenses; the workspace patch also applies to CPAL's existing transitive copy instead of adding a parallel dependency tree. No new unsafe block is introduced in Aède's own audio code.

Admission checks the actual nonblocking `hw:` PCM handle, not just the requested name. It requires exactly one hardware substream rather than assuming additional hardware mixing is transparent. Integer candidates are S16_LE, S24_LE (24 valid bits in a four-byte container), packed S24_3LE and S32_LE, constrained by source precision. Readback establishes the exact rational rate, disabled ALSA resampling, channel count, mono or front-left/front-right mapping and sufficient effective bits. S24_LE uses its documented 24-bit scaling and container alignment rather than treating it as full-width 32-bit PCM. Unknown mapping or format changes are refusals.

The bounded card-control inventory and values are read through the same verified hardware-card control handle retained for event monitoring. The selected device/substream must expose an actual playback channel-map control whose known positions match the PCM mapping; a configured ALSA mapping alone is insufficient. Missing or conflicting maps are refusals. Supported playback gains must report 0 dB and supported playback switches must be enabled. Unknown effects, routing, vendor controls, missing dB semantics and unsupported shapes are refusals; Aède never rewrites controls. Control events, disconnected/suspended state and xruns invalidate the stream rather than triggering recovery.

An owned worker moves only complete, validated frames between the preallocated source ring and a bounded hardware buffer, retaining partial writes and prefilling before explicit startup. A read-back start threshold at ALSA's boundary disables automatic start; a stop threshold at the actual buffer makes programme underruns fail rather than continue silently. It never pads an active programme with silence or calls xrun recovery. Hardware delay separates accepted writes from reported consumption; successful nonblocking drain seals the final consumption. Pause requires hardware support, and unsupported pause fails the stream instead of dropping samples. Source preservation, ALSA-reported consumption and actual digital DAC capture remain three distinct observations.

## Output eligibility and lifecycle

Strict mode needs an explicitly identified output route: host/backend, device, negotiated rate, channel mapping, sample/container format and effective precision. The route must have a supported way to avoid unintended mixing, gain, enhancements and rate conversion. Platform-specific direct/exclusive access is evaluated in the later output step; requesting exclusivity or using CPAL does not itself certify transparency. Unknown eligibility is a refusal, not a silent best effort. Do not automatically change system volume or user audio settings to manufacture eligibility.

Keep software preservation and hardware evidence separate in diagnostics. Record digital-capture acceptance with OS/backend, driver, device/firmware, settings and the tested format grid. Do not extrapolate one accepted device or rate to every device, platform or format. Runtime checks can establish that the requested route/settings are still in use, not continuously observe the DAC's analogue output. A changed or unknown route invalidates strict eligibility and requires revalidation before further source submission.

Compatible consecutive tracks retain the output stream and exact frame order. A format change drains the previous occurrence and validates the next route before submitting its source frames. If the next occurrence is unsupported, report it without skipping it silently; already completed audio and history remain valid. A format restart may have a gap, so no gapless claim is implied for incompatible formats.

Pause/stop/seek are explicit transport actions. Frames intentionally skipped by a user seek are excluded from that visit's preservation/completion claim, just as they are excluded from current listening duration. Transport must not attenuate or fade source samples automatically in strict mode. Device idle/startup silence outside source spans is distinct from the reference programme.

An underrun during an active source span inserts missing audio even when a callback emits safe silence. Mark strict playback failed/incomplete and stop source production outside the callback; never report successful uninterrupted preservation after such an underrun. Reported host xruns follow the same failure rule even when Aède's own PCM queue did not run short. Device loss, unexpected rate/layout changes and ambiguous host errors likewise fail the strict session. Keep callback work bounded: no allocation, locks, logging, filesystem work or recovery negotiation in the callback. No automatic fallback or automatic resume through a changed route is permitted.

## Acceptance before advertising support

Implementation tests must use sibling `*_tests.rs` files and real source fixtures where format behavior is involved. The source and exact-session tests compare complete FLAC/WAV PCM with independent reference digests, retain real 24-bit low bits and cover all signed 16-bit WAV values, 24-bit boundary patterns, read-size invariance, source admission, seeking and MD5/error lifecycle. Adapter tests exhaustively prove the implemented integer-to-floating mappings; session/queue tests cover ordered occurrences, frame attribution, atomic refusals and strict queue failures. These establish software component behavior, not native route or physical output acceptance, and do not update the repository's recorded verification checkpoint.

| Acceptance case | Required evidence |
| --- | --- |
| Decode/reference identity | Native FLAC and WAV fixtures at both depths, mono/stereo, compared with independently established integer PCM. Include silence, signed endpoints, low-bit patterns, impulses and distinguishable left/right content. Preserve a present FLAC MD5 check at EOF and error on mismatch/truncation. |
| Exact adapters | Compare canonical values after same-depth output, packing and widening; cover endpoint alignment/sign handling and all 16-bit values. For 24-bit and any float adapter, establish the full mapping by exhaustive or equivalent rigorous checks, including low bits. No quantizer/dither invocation. |
| Route negotiation | Refuse unsupported rate, fewer effective bits, changed layout, unknown eligibility, conflicting effects and requested fallback. A suitable integer route must not lose to an incompatible preferred float rate. |
| Frame continuity | Exact ordered frames through different decoder/callback block sizes, compatible joins, repeated occurrences, EOF/drain, pause and explicit seek boundaries; no hidden fades or dropped/duplicated frames. |
| Failure behavior | Corrupt/truncated source, preparation failure for a later occurrence, source-span underrun, device loss and route changes produce explicit failure/incomplete status. Preserve valid earlier records and never downgrade strict mode silently. |
| Existing behavior | Explicit DSP processing, original-file transfer, PCM v1, queue controls, cancellation, source identity and history retain their established contracts. The new local default does not modify files or tags. |
| Digital route acceptance | Capture digital output at supported rates/depths and compare every source sample after documented container normalization and fixed capture-latency alignment. Do not fit gain, filter, resample or tolerate sample differences to obtain a pass. Check missing/duplicated frames and idle/load runs; analogue capture and visual rate displays do not prove bit identity. |

Publish claims only within demonstrated coverage. During implementation, report software tests as software evidence and unmeasured device output as unmeasured; mocks cannot establish a physical route. A source MD5 mismatch detected at EOF cannot retract previously played frames, but prevents a successful complete-source verdict.

## Implementation sequence and remaining work

1. Define the strict scope, policy distinctions and acceptance boundaries. The design contract is established.
2. Preserve typed integer source PCM and valid-bit/layout metadata, retaining progressive decoding and FLAC verification. The source API is implemented.
3. Implement the unmodified session, exact software adapters and integer-capable bounded queue. These components are implemented, separately from the existing DSP session, with shared occurrence attribution and queue behavior. They do not introduce another player engine.
4. Integrate native device selection and route eligibility with the existing transport, cancellation, history and diagnostics, using rate-first exact negotiation and explicit refusals. The shared driver and conservative eligibility gate are implemented; direct ALSA adds the first glibc Linux candidate route independently of CPAL. Other platform routes remain refused.
5. Expose the three policies, the without-effects local default and device discovery, with compatible settings and clear diagnostics. This CLI selection is implemented; per-device persistence remains separate work.
6. Run the integrated acceptance matrix and digital capture, then document supported route/format coverage. Do not release a global bit-perfect claim before this evidence exists.

The [native PCM v1 route](../server/playback.md) remains processed `f32le`; extending it to typed exact PCM requires a separately versioned/negotiated client contract and output acceptance. [Original-file adapters](../server/subsonic.md) and [device casting](../server/devices.md) preserve encoded bytes on Aède's side but leave decoding/output to the receiver. Neither is certified bit-perfect by this local contract. A true-peak limiter remains optional [future DSP work](../dsp/premium.md), incompatible with strict sample preservation when it changes the programme.
