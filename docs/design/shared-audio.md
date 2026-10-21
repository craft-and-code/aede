# Shared audio foundation

`crates/aede-audio` is the catalogue-independent PCM foundation used by Aède, Phémios and FlacCompagnon. It reuses `aede-dsp`; it does not introduce another DSP implementation or depend on an application, an analysis engine, a catalogue, an account, HTTP or Tauri.

## Feature boundaries

- No default features or default device/network activity.
- `decode`: progressive local-file Symphonia reader extracted from FlacCompagnon's preview path, with finite complete-frame validation, bounded packets and admission between packet reads. This feature does not pull in the DSP or device backend. Application-specific verified FLAC, Vorbis and external-codec paths remain in Aède.
- `dsp`: access to the existing `aede-dsp` numerical implementation and shared PCM format contracts. Loudness capture/meters are a separate, default-enabled feature of `aede-dsp`, disabled for the minimal player.
- `queue`: Aède's complete-frame bounded single-producer handoff, including its original sibling regression suite, plus Phémios's existing callback transition/reserve policy and the underlying ring types. Queue metadata describes precision; it supplies no bit-perfect device guarantee.
- `output`: optional CPAL single-file `MinimalPlayer`, progressive decoding and the shared bounded queue. It opens no device until explicitly requested, starts paused and has no catalogue, history, normalization, EQ, crossfade or strict-output control.

The minimal player adapts channels and sample rate only as required by the device, using the existing downmixer and bandlimited streaming converter. Same-format PCM is unchanged until device volume and the final output guard. Its ring holds half a second, decoder packets are bounded to 1,048,576 samples, and resampling retains its existing bounded working blocks. It does not retain a track-sized PCM buffer. Control-thread polling publishes device-consumed time and distinguishes real EOF from errors. The audio callback does no allocation, locking, UI emission or logging.

Pause consumes no source frames; mute retains volume. Seek explicitly reopens the decoder and progressively discards a cancellable source prefix, preserving pause/gain. It is not constant-time seeking or gapless output. The optional admitted open/seek APIs recheck the captured owner between preparation stages and before replacing/resuming a prepared stream. A retired prepared value is dropped rather than published; callers with account or playback-generation ownership must supply this admission. Replacement also rechecks admission after dropping/joining the old visit and before resuming the prepared output, since retirement itself can wait for a file read. A cancelled replacement stays paused. Stop/seek joins the retired decoder worker to prevent worker accumulation and can wait for one regular-file read already in progress. Packet admission checks cancellation even when corrupt/non-audio packets are skipped. No forced interruption of OS filesystem I/O is claimed.

## Application ownership

Aède core uses the shared ordinary progressive decoder and PCM formats; its CLI uses the shared handoff. Aède retains verified/exact decoding, rich processing sessions, strict ALSA negotiation, fallback output, history, supervision and the authenticated PCM protocol.

Phémios retains authenticated stream ownership, preloading, per-occurrence accounting, device/UI metering and its continuous output callback. Its transition/reserve policy and ring now come from `aede-audio`; crossfade sample mathematics comes from `aede-dsp::mix`. It continues receiving Aède-processed PCM, without applying normalization a second time. Local CD transport remains independent of an Aède login.

FlacCompagnon's progressive preview decoder is a compatibility adapter to `aede-audio::decode`. Its Tauri player is a command/event adapter to `MinimalPlayer`; no Aède server or account is required. Its analysis decoder and original-integer measurements remain separate and are not normalized or resampled by preview playback. The existing whole-file conversion helper remains available for explicit analysis/conversion workflows, never for the minimal player.

## Source and release integration

Aède keeps ordinary workspace path dependencies between crates in this repository; those paths are contained in a single checkout. Phémios and FlacCompagnon declare `aede-audio` once in their respective `[workspace.dependencies]`, pinned to the public Git revision `d8c0f1a8c134b7366da0983367a9f063527085b7` of `https://github.com/craft-and-code/aede`. Cargo resolves `aede-dsp` from that same revision. Their lockfiles retain both Git source identities, so neither client requires an adjacent Aède checkout or a machine-specific Cargo patch.

The source revision is advertised by the technical tag `audio-core-d8c0f1a8`, which is outside the `v*` release trigger. It exposes the reviewed source without moving a branch or publishing installers. Clients pin the full commit SHA, rather than trusting a movable tag. Phémios's release workflow checks out the same commit for its bundled engine; explicit development engine overrides and the offline preparation policy remain separate from this Cargo dependency.

To update the shared source, first publish the reviewed Aède commit, then update both clients' workspace dependency and lockfiles, together with Phémios's bundled-engine checkout. Verify dependency resolution from independent checkouts with `cargo metadata --locked` and run the relevant library tests. Do not copy the shared implementation into a client, substitute a branch name for the revision, or add a local-only override to a release workflow. A future Aède version can continue using its own workspace crates while clients retain their intentionally selected immutable revision.

## Verification without rebuilding Apps

Run `cargo test -p aede-audio --all-features --locked --offline`, the existing Aède playback tests and the Phémios core tests. The extracted queue/format tests keep their original behavior and count. FlacCompagnon core's `native_preview` harness compiles the actual thin adapter with an inert event sink, without Tauri. Required real encoded-file fixtures remain in each application's existing suites. Library tests, Clippy and Rustdoc do not establish native App execution, other-platform compatibility, physical output fidelity or listening acceptance.

Check feature isolation as well: `cargo test -p aede-audio --no-default-features --locked --offline`, the same command with `--features decode`, and `cargo test -p aede-dsp --no-default-features --locked --offline`. Run Clippy with `--all-targets` for both the minimal and full profiles. The DSP profiling example and loudness-only test fixtures require the `loudness` feature; ordinary device playback does not enable it.
