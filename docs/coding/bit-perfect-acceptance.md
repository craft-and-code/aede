# Strict playback acceptance

This is the acceptance protocol for the [local transparent-playback contract](../design/bit-perfect.md), not a new playback policy. It separates a validated source, exact software handoff, native-platform execution and digital output identity. The [play reference](../cli/play.md#playback-policies-and-strict-output) remains the source of truth for supported options and source admission; do not duplicate its interface here.

## Evidence and present limits

| Boundary | Required evidence | Limit |
| --- | --- | --- |
| Source | Independently established signed integer PCM, rate, valid precision and ordered channels; successful complete decode and, for FLAC, verification of a present audio MD5. | A missing FLAC digest is unknown; WAV has no FLAC digest. Source verification says nothing about the output. |
| Software | Every submitted source frame matches after only documented exact representation changes. Queue, partial-write, seek, drain and failure regressions preserve attribution. | Pure simulations and synthetic captures do not exercise a physical route. |
| Native platform | Compile, link and execute the actual target build on GNU/Linux, including the direct ALSA API, opened-route checks and failure behavior. | A synthetic Linux-configuration metadata harness on another host does not establish this boundary. |
| Digital route | Independently recorded integer PCM from the intended output path, with exact comparison of every programme sample at one fixed frame offset. | One accepted route/format does not certify every host, driver, DAC, rate or future setting. |

Native Linux compilation/linking/runtime and real digital capture remain unverified until their evidence is recorded. The existing implemented default and three local policies do not depend on inventing such a result. Hardware acceptance must remain pending even when source, software and comparison-tool tests succeed.

The [gapless measurement guide](gapless-measurement.md) handles timing markers and analogue observations. Its correlation, clock-drift model and timing tolerance do not prove bit identity. An analogue DAC-to-ADC recording, microphone, displayed sample rate, source MD5 or client acknowledgement cannot replace an exact digital capture.

## Establish the reference and capture route

Use synthetic probe files and a disposable Aède data folder; do not test against or change a personal music library. The reference must be established independently of the playback path under test. Retain the ordered source integer values or their canonical PCM artifact, the manifest and content hashes. For FLAC trials, independently decode or compare the encoded source with the reference and retain its complete MD5 verdict.

Include both accepted source depths, mono/stereo, silence, signed endpoints, low-bit patterns, distinguishable left/right content and boundaries that do not coincide with processing blocks. Full-range endpoint probes belong on a digital capture route with loudspeaker playback isolated; they are not listening material. Select only rates the tested device supports from the contract's 44.1, 48, 88.2, 96, 176.4 and 192 kHz grid. A refused rate remains a refusal, not an omitted successful trial.

Record the capture path explicitly. A software loopback exercises only the software route it observes; it must not be relabelled as a digital capture from a hardware output. A hardware digital return requires an actual compatible digital input/recorder. A DAC with analogue outputs alone cannot supply a bit-identity capture. The recorder must preserve the captured samples, clock/rate, channel association and sufficient integer precision without its own gain, effects, rate conversion or dither. Use the sender's digital clock or an equivalent exact digital return; an independent recording clock with conversion cannot establish source-sample identity. State the observed boundary: a digital tap before a DAC does not validate later DAC processing.

Start recording before playback and retain the complete unedited capture, including the programme's ending. Do not export a lowered-resolution, normalized or resampled copy as the comparison input. Save route/settings, Aède output/error diagnostics, operating system, driver/device/firmware identifiers, binary hash and revision beside the capture. Use generic machine labels and synthetic paths; no account names, credentials or personal-library paths belong in the record.

## Native-platform gate

Before claiming native Linux support, run the repository's required verification on the actual GNU/Linux build host with its normal ALSA development prerequisites, and retain compiler/linker results. Execute device discovery and strict playback against the intended hardware. Confirm that the selected card/device/subdevice is the opened hardware PCM, with one admitted substream, actual channel-map control, exact rational rate, effective precision, disabled rate conversion and neutral supported controls.

Retain refusals as well as successful admission. Plugin/default/mixer routes, unknown mapping, modified or unknown controls, insufficient precision and incompatible rates must not silently choose a different device or policy. Device listing is inventory only; it does not open or validate a route. A successful native build without a suitable device is native build evidence, not runtime or capture acceptance.

The ordinary Linux test gate includes headless refusal coverage but does not open hardware. A separate explicitly ignored lifecycle test is available only on a native GNU/Linux build. Supply all four values for the actual intended device/format:

```sh
AEDE_TEST_ALSA_DEVICE='hw:CARD=0,DEV=0' \
AEDE_TEST_ALSA_RATE=48000 AEDE_TEST_ALSA_BITS=24 AEDE_TEST_ALSA_CHANNELS=2 \
cargo test --locked --offline -p aede-cli explicit_linux_alsa_hardware_lifecycle \
  -- --ignored --nocapture --test-threads=1
```

This invocation opens the named real hardware and sends 200 ms of verified digital silence. It checks held startup, actual consumption/drain and active hardware pause when supported. The device variable omits the CLI's `alsa:` host prefix. Allowed rates are the six declared grid rates, source precision is 16/24 bits and channels are one/two; missing values or an ineligible route fail the selected trial. Preserve the printed format and pause coverage, and confirm that the named test actually executed: zero matching tests or an ignored case is no evidence. This test has not been executed on real Linux here. Silence/lifecycle success cannot establish preservation of nonzero source values or digital-output identity.

## Prepare, run and compare with the helper

The dependency-free [`tools/bit_perfect.py`](../../tools/bit_perfect.py) uses Python's standard library and the existing gapless runner. Preparation writes only synthetic fixtures; comparison reads existing files. Only explicit `run` starts Aède, and none of its commands starts recording. Optional FLAC preparation uses the already supported external FFmpeg encoder; no package is installed automatically.

Prepare a new trial directory:

```sh
python3 tools/bit_perfect.py prepare /tmp/aede-bit-perfect-48k24 \
  --rate 48000 --bits 24 --channels 2 --seconds 3 --flac
```

This writes `whole.wav`, three corresponding WAV parts, optional FLAC parts, `split.m3u` and `manifest.json`, with the complete canonical PCM SHA-256 and source-file hashes. Default probes are quiet deterministic signals with distinguishable channels and meaningful low bits. Omit `--flac` for WAV-only preparation without an encoder. Use `--bits 16` or `--channels 1` for their separate matrix cells; change the directory for every parameter set. Optional `--full-scale` adds signed-endpoint coverage and belongs on an isolated digital route, not loudspeaker playback.

On the native Linux host, identify the actual output before starting the independent recorder:

```sh
target/release/aede play --list-devices
```

Configure the capture for the source rate/channels and sufficient integer precision. Record long enough for the whole prepared programme plus startup and drain. Once the recorder is running, explicitly exercise the selected strict route:

```sh
python3 tools/bit_perfect.py run /tmp/aede-bit-perfect-48k24 \
  --binary target/release/aede --output-device alsa:hw:CARD=0,DEV=0
```

Copy the real device ID, rather than assuming card 0 is suitable. The runner enforces strict playback with normalization off and flat tone, no repeat or shuffle, validates the source manifest and retains separate data/log/result artifacts. It names records by selection/load, for example `split-idle.log` and `split-idle.run.json`, and refuses to overwrite a previous trial. Prepare another directory with the same parameters for each repetition; never delete its previous evidence to reuse a label. Repeat the whole reference with `--whole`, then repeat both whole and split trials under identical bounded load:

```sh
python3 tools/bit_perfect.py run /tmp/aede-bit-perfect-48k24 \
  --binary target/release/aede --output-device alsa:hw:CARD=0,DEV=0 \
  --cpu-workers 2 --io-load --timeout 30
```

The existing [gapless runner](gapless-measurement.md#capture-and-play) defines load-worker and timeout bounds. Load results describe those settings, not a universal NAS or real-time performance budget.

Compare an unedited integer PCM WAV capture:

```sh
python3 tools/bit_perfect.py compare /tmp/aede-bit-perfect-48k24/manifest.json \
  /tmp/aede-bit-perfect-48k24/split-idle.wav --kind digital-device \
  --output /tmp/aede-bit-perfect-48k24/split-idle.comparison.json
```

Use `--kind digital-loopback` for a software return and `--kind synthetic` for generated comparison inputs; the tool cannot infer physical provenance from a file. It accepts uncompressed integer PCM WAV captures at 16/24/32 bits with exactly matching rate/channels and sufficient precision. Segmented `LIST/wavl` and `slnt` audio is refused rather than silently ignored. Floating or analogue captures, narrowing and changed rates are outside this exact comparator.

Automatic alignment searches a bounded startup interval, five seconds by default, for an exact 64-frame source prefix. Ambiguous alignment refuses a comparison. Supply a known `--offset-frames N` or a larger `--search-frames N` when justified, up to fifteen seconds in frames; these are mutually exclusive. One selected offset must still satisfy the complete-programme comparison. Recording frames outside that programme must be exact digital zero, so an extra nonzero repeated tail or unrelated audio cannot hide outside the matched span.

The JSON result separates `comparison_passed` from `hardware_acceptance`, which always remains false in the helper. It retains file/reference hashes, alignment and mismatch/missing-tail evidence. A passing generated-source self-test validates comparison behavior only; a passing real capture still needs the native-platform, route, logs and repeated-trial record before a scoped hardware claim.

## Implemented software matrix

The mandatory sibling CLI acceptance tests drive the existing strict transport through a simulated representation sink. Their [independent source fixtures](../../crates/aede-cli/tests/fixtures/bit-perfect/README.md) contain 4233 frames, crossing the 4096-frame boundary, with silence, signed endpoints, impulses, byte/low-bit patterns and distinguishable channels. Reference libFLAC independently encoded and decoded the native FLAC artifacts with complete byte identity; the WAV oracle is constructed from known signed source values, not copied from Aède's decoded output.

The matrix covers native FLAC and ordinary PCM WAV at both 16/24 bits, mono/stereo and the six named rates. Compatible repeated joins exercise admitted signed 16-bit, packed 24-bit and full-width 32-bit software representations, including exact widening and padding. Seek cases compare the exact suffix and keep history incomplete even when a nonzero source-frame offset rounds to a zero-millisecond display. High-resolution MD5 failures preserve preceding complete history, retain the failing partial visit and never start the later queued source.

These are integrated software results over a simulated sink. They do not establish that ALSA negotiated any matrix cell, that a native Linux build ran, or that a recorder observed a DAC. Keep comparison-tool tests, native-platform execution and physical route records separate.

## Exact comparison rules

One constant capture-frame offset may account for fixed startup latency. Apply the same offset to the whole programme and every join. Do not independently align tracks, fit gain, interpolate, resample, remove clock drift, ignore changed low bits or expand a mismatch tolerance. These operations can conceal the changes that strict playback is meant to detect.

Compare signed sample values with rate, precision and channel association preserved. If a source has `b` valid bits and a full-width captured PCM format has `B >= b` bits, exact widening is `captured = source × 2^(B - b)`. Validate every padding bit introduced by widening; right-shifting and discarding nonzero captured low bits is not an exact comparison. An insufficient-precision capture is unsuitable, not proof that playback failed. Packing or byte-order normalization is allowed only when its adapter is documented and invertible.

Require the full ordered programme to match, including its first/last samples, intentional repeated occurrences and the samples around every join. A matching prefix, isolated marker, source digest or frame total cannot establish complete identity. A changed sample, swapped channel, insertion, deletion, missing tail or inconsistent rate prevents acceptance. Pre/post recording time outside the declared programme is distinct from source frames; retain it and require exact digital zero there rather than trimming the evidence silently.

Capture metadata cannot establish its physical provenance. The comparison report must state the supplied capture kind and retain physical acceptance as pending until the route record, logs and trial matrix are reviewed together. A synthetic self-test validates the comparator only.

## Trials and acceptance record

For each claimed source format, valid precision, channel layout and supported rate, repeat at least three idle and three bounded-load trials with a fresh capture and result record. Keep the load settings, capture route and comparison rules fixed. Do not promote an untested matrix cell from another cell's success.

| Trial | Required observation |
| --- | --- |
| Complete file and compatible ordered queue | Exact samples, occurrence order, repeated entries and final tail; no programme underrun, route change or hidden conversion. |
| Format change | Previous programme drains, next route is validated, and its source samples remain exact; no gapless claim across incompatible formats. |
| Pause/resume | Supported hardware pause retains programme frames; unsupported pause produces the documented strict failure instead of restart/fade. |
| Explicit seek/Next/Stop | Intentionally skipped/discarded spans stay outside completion and duration; no successful uninterrupted-playback claim for the whole source. |
| Corrupt/truncated source or MD5 mismatch | Terminal source failure, incomplete failing occurrence, valid preceding histories retained, no automatic decoder/sink fallback. |
| Underrun, device loss or changed controls | Terminal preservation failure; no silence substitution, automatic recovery or false completion. |

Retain the independent source artifact/manifest, binary revision/hash, capture kind and hardware provenance, route readback/settings, complete captures, comparison results, Aède diagnostics and per-trial load record. Publish only the exact accepted host/driver/device/settings and format cells. Changed hardware, driver, routing, controls or relevant code requires revalidation; passing software tests alone does not renew a physical-route claim.

For the current implementation and remaining work, use [current state](current-state.md), the [bit-perfect design](../design/bit-perfect.md#implementation-sequence-and-remaining-work) and the [output guide](../dsp/output.md). Keep actual acceptance records separate from this stable protocol.
