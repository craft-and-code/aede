# Aède and FlacCompagnon audio processing comparison

Reviewed on 2026-10-04 against the hardened Aède working tree and FlacCompagnon's local 0.9.6 source at commit `c07e65a651b41e9ce6036ab3979975ffcec61cb2`. Aède currently pins the published FlacCompagnon `v0.9.5` analysis core. This review does not update that pin, edit FlacCompagnon, add dependencies, or establish physical playback or meter certification.

## Assessment

The implementations have complementary responsibilities. Aède has the more complete playback-processing chain. FlacCompagnon has the more complete forensic/file-analysis chain, including several measurements absent from Aède's playback meter. Replacing all FlacCompagnon analysis with `aede-dsp` would remove useful behavior. Sharing selected PCM algorithms is worthwhile after their contracts and reference tests are made explicit.

FlacCompagnon also has a separate CPAL preview player and PCM conversion helpers. Those playback/conversion paths use simpler resampling than Aède and are good candidates for sharing Aède's bandlimited conversion. They are distinct from FlacCompagnon's authenticity detectors, which must continue to inspect original samples before any playback DSP.

## Capabilities and ownership

| Capability | Aède | FlacCompagnon | Sharing decision |
| --- | --- | --- | --- |
| Playback gain and headroom | Smooth gain, metadata-selected normalization, peak-aware static cap, final sample guard | Preview volume/mute; no equivalent normalization chain | Share the sample algorithms if wanted; retain application policy separately |
| Tone controls | Optional bass/treble shelves, exact flat bypass | No comparable preview EQ found | Optional preview processing; never place it before authenticity analysis |
| Rate conversion | Stateful bandlimited FFT/sinc, finite-tail drain, exact cumulative frame counts, numerical refusal | Linear interpolation in preview fallback and MP3/Opus conversion | First candidate for replacement, preserving encode frame-count and cancellation contracts |
| Channel handling | Explicit supported speaker masks and conservative stereo downmix | Preview nearest-channel remap, mono duplication | Share a labelled matrix only when the decoder supplies trustworthy speaker positions |
| Final integer conversion | Continuous TPDF quantization for native integer output | Service conversion rounds and clamps without TPDF | An explicit conversion policy is needed before adopting dither; do not silently change lossless export behavior |
| Integrated loudness | `ebur128` programme histories, mono/stereo, combination across format sections | Custom K-weighted gating, mono/stereo, per-file reporting | Numerically close on the tested grid; preserve distinct file/programme contracts |
| True peak | `ebur128` with finite-signal tail; unknown at 192 kHz and above | Fixed 4× 48-tap FIR, per-channel, all sample rates | Estimates differ; resolve tail/high-rate/invalid-input policy before selecting one implementation |
| Loudness range, momentary/short-term maxima | No dedicated playback API | LRA and real-window M/S maxima with time locations | Preserve in FlacCompagnon; optional shared measurement API can expose these later |
| Playback visualization | 2048-frame FFT, 24 display bands, grouped into 12 terminal columns | Preview RMS animation; offline 8192-point accumulated spectrum | Display and forensic spectra are different products; retain their distinct semantics |
| Authenticity and integrity | Imports/runs FlacCompagnon; separate current decoded FLAC MD5 verification during playback | FLAC verification, effective bit-depth/grid evidence, spectral/MDCT/lattice transcoding analysis, clipping, stereo/phase, DC offset, clicks/dropouts and related evidence | Keep the analysis chain and attribution; `aede-dsp` is not its replacement |
| Device output | Native CPAL transport in the CLI, bounded PCM ring, callback diagnostics; explicit ffplay fallback | CPAL preview transport in Tauri | Device transport is outside `aede-dsp`; reusing DSP alone does not replace either player |

Aède already preserves complete imported FlacCompagnon report entries in `FileAnalysis::source_data`, including measurements it does not interpret. The existing regression covers LRA, M/S maxima and future nested fields through conclusions storage. The absence of a dedicated playback API therefore does not erase imported evidence, and does not prevent current integrated-loudness normalization.

## Direct numerical comparison

A temporary Rust harness used FlacCompagnon's public `LoudnessMeter` and `TruePeak` source modules, and the hardened Aède release library. It fed identical finite `f32` PCM to both implementations without decoding, playback, devices or network access. Rust 1.99 compiled the harness; no Cargo manifest or lockfile was changed. This is a focused synthetic comparison, not a complete compliance campaign or a speed benchmark.

The integrated-loudness grid contains twenty cases: mono/stereo at 44.1, 48, 96, 192 and 384 kHz, with both a three-second 1 kHz sine at −23 dBFS peak per channel and a twelve-second mixed-frequency programme. The mixed programme uses three-second level sections at −20/−26/−34/−22 dB, frequencies 80 Hz, 997 Hz plus 103 Hz per channel, and 7 kHz with weights 0.5/0.3/0.2; the second channel has an additional 0.7 gain. Aède receives 1031-frame blocks; FlacCompagnon receives individual complete frames.

| Sample rate | Largest absolute integrated-loudness difference on these four cases |
| --- | ---: |
| 44.1 kHz | 0.00336 LU |
| 48 kHz | 0.00000 LU |
| 96 kHz | 0.01972 LU |
| 192 kHz | 0.03007 LU |
| 384 kHz | 0.03536 LU |

All twenty cases remain below the 0.1 LU comparison budget. The implementations use different filter construction and gating storage; this result does not establish equality on every signal, gate threshold, duration or sample rate. Each project's existing independently synthesized EBU reference cases remain useful and must survive a migration. FlacCompagnon additionally supplies an opt-in FFmpeg comparison for integrated LUFS, LRA and M/S maxima; it was inspected, not rerun here.

True peak is not numerically interchangeable. A one-second quarter-rate sine of amplitude 0.98 at phase π/4 gives a sample peak near 0.693, FlacCompagnon's fixed FIR estimate near 0.97135, and Aède's finite-signal estimate near 0.99175 below 96 kHz and 0.99184 at 96 kHz. These are estimates from different reconstruction filters; this comparison alone does not select a universally more accurate filter. At 192 and 384 kHz Aède explicitly reports no oversampled estimate, while FlacCompagnon continues to emit its fixed 4× estimate.

### Confirmed end-of-file peak difference

A one-second otherwise silent signal with one full-scale sample in its final frame exposes an unfinished filter tail in FlacCompagnon. Its meter reports approximately 0.000363, whereas Aède's finite-tail snapshot reports 1.0 at 44.1, 48 and 96 kHz. FlacCompagnon's `StreamAnalyzer::finish` reads `TruePeak::peak` directly without resolving pending FIR outputs. There is no zero-extension finalizer in that meter. The same direct-meter endpoint case was exercised at 192 and 384 kHz; FlacCompagnon still reports the small pending result while Aède retains its documented unknown high-rate policy.

This finding justifies a targeted FlacCompagnon follow-up with a failing complete-analysis endpoint regression, finite-tail handling, unchanged loudness windows and report compatibility. FlacCompagnon was not modified in this task.

For Aède, the imported true-peak estimate used to set headroom could consequently be lower than a known imported sample peak. A new failing regression reproduces the unsafe gain decision; the playback derivation now floors a present, finite true-peak estimate at a present, finite sample peak. Higher true peaks remain unchanged. Missing true peak remains unknown even when a sample peak is known. Original report values and attribution are untouched. This conservative bound prevents a known sample from justifying excessive gain; it does not repair or certify the external interpolator.

## Preview and conversion differences

FlacCompagnon's native preview first requests source rate/channels, then falls back to whole-file decoding and linear interpolation with nearest-channel remapping. Its progressive input grows a `Mutex<Vec<f32>>`; its CPAL callback also locks the position and emits Tauri level/position/finished events. These choices satisfy its current preview contract but differ from Aède's bounded-ring native transport, callback atomics and background presentation. Merely importing `aede-dsp` would not migrate the callback or make preview joins gapless.

FlacCompagnon's MP3/Opus services also use linear interpolation without an explicit anti-alias filter. Lossy encoding does not make upstream aliasing harmless. Reusing Aède's bandlimited converter is a concrete quality improvement to evaluate with independently generated high-frequency/downsampling fixtures and encoder-duration checks. Keep original-file preservation and cancellation behavior. Unrequested EQ/normalization must remain disabled in conversion, and integer/dither behavior must be separately specified for lossless exports.

## Dependency, version and license constraints

`aede-dsp` depends only on `biquad`, `ebur128` and `rubato`; it does not depend on the catalog, FlacCompagnon, devices or HTTP. Consequently `flaccompagnon-core → aede-dsp` would be acyclic today. Depending on `aede-core`, `aede-cli` or `aede-server` from FlacCompagnon would create the wrong reverse dependency: Aède's core already depends on FlacCompagnon's core.

Aede's workspace and DSP crate use MPL-2.0, with Rust 1.89 and edition 2024. FlacCompagnon's reusable core is also MPL-2.0; its services/application workspace defaults to MIT and edition 2021. MPL modules must retain their notices and corresponding source obligations when distributed. Edition differences alone do not prevent a dependency, but adopting the shared crate would introduce an explicit Rust 1.89 minimum. FlacCompagnon does not currently declare a minimum Rust version and its workflows select moving stable Rust; its own platforms/release matrix would need checking rather than inferring compatibility from Aède's CI.

Aède pins FlacCompagnon `v0.9.5`; the inspected local source declares 0.9.6 and is ahead of that tag. Future shared-layer work should use stable tagged releases or a published standalone crate, with fixed source revisions and coordinated report/method versions. It must not make either project require a development checkout of the other. Adding the dependency or publishing/extracting a crate is a separate approved change.

For measurement-only reuse, pulling all resampling/EQ dependencies into an analysis core may be unnecessary. Keep module selection or separately packaged measurement/processing layers in the migration design; do not tie analysis to a device or catalog runtime.

## Recommended migration sequence

1. Keep FlacCompagnon's forensic chain. Correct and independently verify its finite true-peak tail before changing measurement ownership.
2. Define shared PCM contracts: finite complete frames, known channel roles, source/output measurement distinction, unknown high-rate peaks, format changes, finalization and numerical errors.
3. Share bandlimited rate conversion in FlacCompagnon's preview and lossy-conversion services first, with exact frame/duration, anti-alias and whole-versus-split tests.
4. Compare the measurement APIs against both projects' reference suite. Preserve LRA, M/S maxima/times, malformed-stream refusal and source/report compatibility before deleting duplicate meter code.
5. Extract or publish only the reusable PCM layer. Keep decoders/forensic decisions in FlacCompagnon, playback normalization/queue policy in Aède, and CPAL adapters with each client.
6. Migrate callers incrementally. Remove duplicate implementations only after their old observable behavior or an explicitly documented change is covered.

This task performs the comparison and the conservative Aède import correction. It does not add LRA/M/S playback displays, migrate FlacCompagnon, change export audio, or add optional effects.

## Verification

The imported-peak regression failed before the correction and passed afterward. Rust 1.89 runs passed all four `playback::loudness::tests` and all twenty-two `playback::gain_plan` tests. Warning-free all-target Clippy passed for `aede-core`, and formatting/diff whitespace checks passed. The standalone numerical harness completed its twenty loudness comparisons and ten true-peak observations. No FlacCompagnon test suite, hardware playback, complete Aède workspace gate or cross-platform release build was run by this comparison subtask; their existing checkpoint records remain separate.

## Implementation evidence

Aède:

- [DSP API and dependencies](../../crates/aede-dsp/README.md), [sample processing](../../crates/aede-dsp/src/lib.rs), [rate conversion](../../crates/aede-dsp/src/rate.rs), [channel mapping](../../crates/aede-dsp/src/channels.rs), [TPDF](../../crates/aede-dsp/src/dither.rs).
- [Programme loudness](../../crates/aede-dsp/src/loudness.rs), [finite peak tail](../../crates/aede-dsp/src/peak_tail.rs), [output snapshots](../../crates/aede-dsp/src/meter.rs), [reference loudness cases](../../crates/aede-dsp/src/loudness_reference_tests.rs).
- [Imported loudness playback derivation](../../crates/aede-core/src/playback/loudness.rs), [import regression](../../crates/aede-core/src/playback/loudness_tests.rs), [complete source-report preservation](../../crates/aede-core/src/analysis.rs), [preservation regression](../../crates/aede-core/src/analysis_tests.rs).
- [Playback transport ownership](../design/playback.md), [current signal-validation record](dsp-review.md).

FlacCompagnon source paths below are relative to that project's root at the reviewed local revision. Its publication was not checked, so these are source references rather than assumed public commit links:

- `core/src/analysis/analyzer.rs:458` and `:493`: finalization and immediate true-peak snapshot; `core/src/analysis/truepeak.rs:25`, `:127` and `:148`: FIR coefficients, input and snapshot without a finite-tail finalizer.
- `core/src/analysis/loudness.rs:112`, `:205` and `:249`: integrated/M/S/LRA state, integrated gates and analysis-only LRA tail; `core/src/analysis/loudness_peaks.rs:31`: complete-window maximum and time location.
- `core/tests/unit/analysis/loudness.rs:46`: synthetic reference cases; `core/tests/loudness_reference.rs:9`: opt-in FFmpeg comparison.
- `src-tauri/src/playback.rs:460`, `:527`, `:561` and `:589`: whole-file resampling fallback, callback locks/events and linear/remap helper; `services/src/convert/pcm.rs:70` and `:104`: conversion resampling and integer rounding.
- `core/Cargo.toml:8`: reusable-core MPL-2.0 license; `Cargo.toml:11`: application workspace version, edition and default license; `.github/workflows/ci.yml:37`: moving stable toolchain.
