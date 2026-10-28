# Native DSD input

## Scope and shared boundary

`aede-audio`, with its existing `decode` feature, reads uncompressed DSF and DSDIFF (DFF) as progressive PCM without spawning FFmpeg or another program. No new product dependency is introduced. The source is read-only; catalogue, accounts, transport and analysis verdicts remain outside this decoder. Native DSD/DoP device transport is a separate output capability from eliminating decoder subprocesses.

The separate unpublished [native DoP output](dop-output.md) now reuses the raw container reader and transports original bits from the CLI without these PCM filters. It requires an explicit compatible DAC; existing PCM consumers keep the profiles below.

The initial profile admits the 44.1 kHz family at DSD64 (2,822,400 one-bit samples/s) and DSD128 (5,644,800), mono or stereo. DSF LSB-first and MSB-first byte declarations are respected; DFF byte interleaving and declared stereo order are verified. DSF block padding and partial final bytes never extend the track. DFF duration comes from bounded audio extents. DST-compressed DFF, SACD ISO, other rates and multichannel are explicitly outside native admission.

`dsd::supports_pcm_profile` is the common rate/channel capability used by the decoder and FlacCompagnon's native/external selection. It does not validate a file, infer compression or admit a payload by itself: the chosen reader must still parse and validate the actual source. This keeps analysis and spectrogram selection in step with the shared decoder without copying its admitted-rate list.

Container references: [Sony DSF 1.01](https://dsd-guide.com/sites/default/files/white-papers/DSFFileFormatSpec_E.pdf), [Philips DSDIFF 1.5](https://www.sonicstudio.com/pdf/dsd/DSDIFF_1.5_Spec.pdf). The implementation derives its parsing and filters from specifications and first principles, without importing another decoder implementation.

## Distinct PCM profiles

- **Measurement**: `decode::PcmStreamDecoder::open` or `dsd::Profile::Measurement` produces wideband PCM at the source clock divided by eight: 352.8 or 705.6 kHz. A normalized, 256-tap Blackman-windowed sinc, nominal cutoff 0.045 times the DSD clock, rejects decimation images while retaining the 20–75 kHz region used by authenticity checks. It applies no loudness normalization, EQ, volume or listening low-pass. Its response is not identical to the previous external decoder; detection thresholds and real-file comparisons remain acceptance work.
- **Listening**: `open_for_playback` or `Profile::Listening` adds a 30 kHz low-pass and decimates to 88.2 kHz. The second FIR has 256 taps for DSD64 and 512 for DSD128. Existing device-rate/channel adaptation happens afterwards. The minimal player and Aède's file-playback reader select this profile automatically. It is converted PCM, **not native DSD, DoP or a DSD bit-perfect route**.

Linear-phase lookahead is primed and drained internally. Output has exactly the ceiling of the source sample count divided by the total decimation factor, without appending the FIR tail. Edges use balanced DSD silence / zero PCM outside the source rather than DSF padding as music. Original DSD properties stay separate from derived PCM rates and carry no invented PCM precision or MD5 result.

## Bounds and lifecycle

Parsing uses fixed headers, checked chunk/parent/file extents and a shared 65,536-chunk budget for DFF. The required DFF form/version prefix is shared by native and compatibility callers through `dsd::read_dff_header`: the unique version chunk comes first, the main version is supported, reserved bytes are zero and sound properties precede audio. Compatible minor additions remain admitted. Unsupported or duplicated structural properties fail explicitly. Source and metadata share an opened regular-file descriptor; optional ID3 reads are bounded by their container and the existing metadata budget.

Input buffering holds at most 4,096 DSD bytes per channel. Both blocked DSF and interleaved DFF use block reads, never one OS read per DFF audio byte. The first FIR shares a roughly 64 KiB byte-contribution table; channel history, listening FIR state and each output chunk remain bounded independently of track length. A packet contains at most 2,048 PCM frames. Work happens on decoder workers, never the device callback.

Admission is checked during read/filter work and before chunk publication. A read or admission error permanently retires the DSD reader. Drop owns no subprocess or extra thread. Caller generations, pause, stop, seek and account ownership remain in existing players/transports; an in-progress OS file read cannot be forcibly interrupted. Seeking still progressively discards a source prefix, using **derived PCM frames**, not DSD bits.

## Coverage matrix

This matrix is a checklist of inputs and expected behavior, not another runtime configuration.

| Input / action | Expected behavior |
| --- | --- |
| DSF LSB/MSB and DFF with the same independently encoded signal | Same wideband samples and channel order |
| DSD64/128, mono/stereo | Genuine source properties; correct derived profile rate |
| Block boundary / partial byte / EOF / repeated EOF | Exact frame count, no padding played, stable completion |
| Known 1 kHz and 20 kHz sigma-delta tones | Preserved level in both profiles |
| Known 60 kHz tone | Preserved for measurement; strongly rejected for listening |
| Constant positive / negative bitstreams | Correct polarity and unity DC gain, no normalization |
| Truncated / overflow / duplicate / unsupported layout | Explicit error without panic or large metadata allocation |
| Cancel / late truncation | No further successful publication from a retired decoder |
| Native playback seek and authenticated PCM transport | PCM-rate units, finite samples, existing ACK/owner contract |

Fixtures under `crates/aede-audio/tests/fixtures` contain real DSF/DFF-encoded synthetic audio, with no copyrighted recording. Their provenance and regeneration method are documented there. Tests construct additional malformed and boundary files separately from production code. Hardware listening, independent real-world DSD-library comparisons, target-platform throughput and DAC acceptance are still required; fixture tests do not establish them.

Both the DSD64 mono and DSD128 stereo reference pairs cover the complete native path: exact listening-frame count, seek in PCM frames, authenticated server format/consumption ACKs, wideband analysis and Rust spectrogram rate/labels. Stereo uses different known tones per channel. Tail and cancellation cases also cover both rates, channel separation across buffer wraps, partial final DSF bytes, listening-filter drain and terminal late read failure. A faster-than-real-time synthetic decode on one host is a software throughput observation, not a guarantee for a DAC, another platform or concurrent collection processing.

## Specification and signal-processing qualification

File-format conformance, conversion fidelity and native-device transport are separate contracts. Sony DSF 1.01 defines blocked channels, bit declarations, source sample counts and zero block padding. Philips DSDIFF 1.5 defines big-endian chunks, even padding, the required version/properties and MSB-first interleaved channel bytes. The parser checks those structural fields for its admitted profile, skips bounded unknown DFF chunks and refuses unsupported compression/layouts. Neither specification prescribes this project's PCM FIR design or certifies its listening quality. An unsupported valid profile is a capability limit, not proof that its source is malformed.

The signal-processing basis is low-pass filtering **before** each decimation, rather than averaging or discarding DSD bits. [Analog Devices MT-022, pp. 4–6](https://www.analog.com/media/en/training-seminars/tutorials/MT-022.pdf) explains the anti-alias and noise-shaping requirement. [Reiss and Sandler, DAFx 2004](https://dafx.de/paper-archive/2004/P_372.PDF) concerns processing a one-bit stream directly; it does not make PCM conversion a native-DSD output route, nor provide a normative PCM filter. Our independently derived FIR stays in `aede-audio/src/dsd/filter.rs`, shared by both clients.

`filter_tests.rs` evaluates the actual FIR transfer functions independently of bit packing. The following are project engineering bounds, not SACD or ISO certification thresholds:

| Stage | Passband contract | Stopband contract | Current analytical observation |
| --- | --- | --- | --- |
| DSD64/128 to source-clock/8 measurement PCM | Less than 0.01 dB deviation over DC–75 kHz | Below −75 dB from the output Nyquist to the source Nyquist | Stopband peak approximately −79 dB on the checked grid |
| Measurement PCM to 88.2 kHz listening PCM | Less than 0.01 dB deviation over DC–20 kHz | Below −90 dB from 44.1 kHz to the input Nyquist | Approximately −97 dB at 44.1 kHz; 20 kHz deviation below 0.001 dB |

These observations evaluate the f64 coefficient response on a finite grid (50 Hz passband steps and 4,097 stopband points). They are not measurements of complete f32 decoder THD+N, integrated aliased noise, a real modulator or a DAC. The existing encoded-tone tests separately exercise the bit reader, convolution, phase/drain and channel pipeline. Before calling the decoder production-qualified, retain independent higher-order-modulator and real-file references, swept/multitone signals, impulses, silence/idle patterns, near-overload levels and full end-to-end noise/distortion measurements for both rates and profiles. Compare levels and alignment explicitly; sample-for-sample identity with a different FIR is not expected. Authenticity thresholds need their own real-library validation after a filter change.

The amplitude convention is explicit: a source bit maps to −1/+1 and the FIR has unity DC gain. There is no hidden +6 dB boost or clipping in the measurement profile. A 50% modulation sine therefore has approximately 0.5 PCM peak (−6.02 dB relative to PCM peak 1), not an automatically normalized peak of 1. [Weiss's public Saracon manual, §3.3.4](https://weiss.ch/wp-content/uploads/2023/02/saracon-manual-win.pdf) documents a +6 dB DSD-to-PCM convention and warns about overload; that convention is not applied silently here. No vendor coefficients or implementation are used. Do not label these raw PCM levels as SACD-calibrated dB without an explicit conversion/reference convention.

Full format coverage remains a separate sequence:

1. Qualify higher source clocks and the 48 kHz family before extending the shared capability predicate and its listening/measurement filters.
2. Add multichannel layouts with explicit channel IDs/masks and independent channel references. DSF and DFF have different orders; channel count alone is insufficient. DSDIFF's historical ITU reference does not replace a current layout review; [ITU-R BS.775-4](https://www.itu.int/rec/R-REC-BS.775-4-202212-I) is the current published recommendation.
3. Treat DST decompression, SACD/ISO parsing and DoP/native-DSD device transport as distinct implementations and acceptance scopes. DSDIFF 1.5 §3.4 describes DST frames and CRC; this raw-bit decoder does not decode them. No full Scarlet Book or ISO conformance is claimed from the public container documents or engineering tutorials.

FlacCompagnon's `decode/dsd_native.rs` is only an analysis visitor over Aède's measurement decoder. Its spectral authenticity checks remain analysis-domain code. Its broader legacy DFF metadata parser still serves compatibility profiles, while sharing the mandatory DFF prefix reader; it is not another DSD-to-PCM FIR. Moving new common decoding/filter behavior into that visitor would violate the shared boundary.

## Integration and release order

Aède's catalogue recognizes DSF/DFF by signature and scans their extensions, reads bounded optional ID3 and retains the genuine one-bit clock/depth. Ordinary playback reuses the shared listening decoder and existing PCM transport, so Phémios needs no second DSD decoder or new account mechanism.

FlacCompagnon's native DSD adapter feeds the existing analysis and Rust spectrogram builders through the measurement profile. Its existing external path remains optional for rates/layouts outside this first native profile. The candidate Aède spectrogram adapter now reuses the same wideband measurements and Rust renderer. Optional legacy codec decoding remains separate; see [spectrograms](../spectrograms.md) for publication, freshness and delivery boundaries.

Full FlacCompagnon analysis and spectrograms both choose DSD from the source signature, so a wrong extension cannot send it to the FLAC/PCM path. Native full analysis forwards its cancellation callback through filtering and checks it before exposing completed measurements. Already-cancelled DSD requests skip fingerprints; a fingerprint read already in progress remains non-interruptible. Aède's cancelled native spectrogram keeps the previous image and never attempts the external compatibility decoder as a cancellation fallback.

That signature classification also keeps metadata-only FLAC MD5 checks inapplicable to DSD, even when its name ends in `.flac`. FlacCompagnon's public DSD decoder and optional legacy visitor reject channel/rate hints that do not match the actual source header. Native decoding does not implicitly enable DSD conversion in its services workflow; both declared DSD extensions and recognized DSD content remain refused there.

The legacy DSD visitor resolves its local input path before invoking FFmpeg, so a relative protocol-looking name cannot select another file. The shared `pcm_wave` reader checks the bounded self-describing float WAV header; FlacCompagnon compares its actual rate/channel count with the admitted source properties before delivering samples. Aède's measurement fallback reuses that same reader. Late cancellation at the PCM EOF or process completion remains an error rather than a completed visit. These checks do not lock a source against concurrent filesystem edits; spectrogram callers retain their separate before/after source checks.

`dsd::inspect_dsf` shares the DSF structural checks with compatibility callers: actual file size, padded channel-block extent, source sample count and metadata bounds are validated even at higher clocks or standard multichannel layouts. This metadata-only inspection does not expand native PCM admission. FlacCompagnon also walks DFF outer chunks beyond sound properties to validate audio extents and complete uncompressed channel frames before its optional external path. A decoder recovering audio from an invalid container cannot grant it a completed DSD badge. DST remains header-only. Uncompressed DFF metadata also derives its exact source count from the validated audio extent. The compatibility visitor requires the ceiling of that source count divided by eight PCM frames: a successful FFmpeg exit with a missing DSF partial final byte is an explicit failure, not a completed analysis. Cancellation is checked after pipe reads and before every sink publication, including buffered frames. Header-only and unsupported-measurement paths also recheck cancellation after structural inspection, before retaining a successful result or unverified badge.

The browser client does not gain a DSF/DFF decoder from this native-file integration; browser DSD playback remains a separate transport/WASM adaptation. Existing saved analyses remain historical measurements; this change does not silently erase or relabel them. Recalculate explicitly when comparing decoder responses.

These are **unpublished source changes**. Client build preparation follows the latest **published stable** Aède, not a local sibling tree. Verification of the candidate FlacCompagnon adapter therefore uses an isolated temporary Cargo workspace pointed at this candidate shared crate, without changing production pins. Publish the shared Aède source first, refresh and publish FlacCompagnon, then refresh Aède's analysis dependency and client engine source. Until those selections are updated, an installed App or a normal client build using the previous stable version does not gain this decoder automatically. Do not commit local dependency paths or silently change stable selection to a development branch.


## Catalogue inspection and presentation

`dsd::inspect_source` validates uncompressed container extents without granting native PCM admission. Higher clocks and structurally recognized DSF channel layouts therefore retain their genuine frequency, channels, one-bit depth and duration in the catalogue, even when `dsd::inspect` and the PCM decoder refuse that profile. Unsupported compression and unknown DFF channel orders remain explicit errors. Catalogue bitrate arithmetic uses a wide intermediate; an unsupported high clock cannot panic through multiplication overflow. Only integral power-of-two multiples of 44.1 kHz from DSD64 receive a `DSDn` label; other clock families remain `DSD` with their actual frequency.

Track-detail JSON adds derived `properties.dsd` facts: source label, nullable `native_pcm`, nullable listening/measurement PCM rates, and a stable refusal reason. They come from the shared profile predicate and listening clock, without a second client rate matrix or persisted capability flag. Incomplete source properties mean unknown support. These scanned facts are not a fresh-file integrity, physical-device or optional external-decoder guarantee. Actual opening still validates the retained file.

Phémios displays source facts separately from converted PCM, explicitly explains conversion rather than claiming native DSD/DoP/bit-perfect output, and distinguishes unsupported native conversion from an older engine that supplies no capability object. Existing compatibility playback remains available. Source/API tests and local component previews are separate from rebuilding or running the installed Apps.
