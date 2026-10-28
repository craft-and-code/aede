# Local native audio acceptance

`prepare.py` creates a disposable Cargo test crate over explicitly selected Aède, FlacCompagnon and Phémios source checkouts. Only library/test executables are built: no Tauri App, frontend bundle, optimized CLI, embedded engine, output device or installer. The production manifests/lockfiles are not rewritten. The temporary crate unifies the shared audio/DSP source and uses the real public file, preview/spectral and CUE APIs.

The acceptance suite checks original ramp/silence PCM, old/modern frame-edge seeks and EOF, CRC failure before publication, native spectra and complete CUE playback in a child with empty PATH. Explicit preparation can download the pinned public `sh3.ape` historical reference. Source SHA-256 and independent FFmpeg PCM SHA-256 are in the canonical shared fixture provenance; its PCM MD5 is checked before exact old-3.97 seek and consumer comparisons. The recording is temporary reference data, never redistributed as project-owned audio.

Pass `--aede`, `--flaccompagnon`, `--phemios` and `--output` paths to `prepare.py`. Then freeze the test-only lock with Cargo metadata. `run.py` takes those source paths, `--acceptance` and an explicit Rust `--target`; `--offline` retains the existing cache. Set `NATIVE_APE_FIXTURES` to the shared native-extra corpus and `NATIVE_APE_REFERENCE` to the verified reference. Reports retain compiler, source commits/dirty state, test-lock hash and executable architecture. PE, ELF and Mach-O are distinguished; executed musl tests additionally require no interpreter/shared-library dependencies.

A cross-compiled archive or linked probe is compilation/link evidence only. `link_probe.cpp` retains all three SDK C ABI entry points for such checks; it is not a decoding or execution result. Windows GNU does not certify MSVC. Native Windows/Linux execution requires an appropriate local environment; no hosted runner, remote branch, release or App build is started by these tools. On this host the three-consumer runtime tests run on macOS. Unrelated working-tree changes must remain excluded from source commits.

For example, from the Aède checkout (replace `/work` with explicit local checkouts and choose the host Rust target):

```sh
python3 tools/native-platform/prepare.py --aede /work/aede --flaccompagnon /work/FlacCompagnon --phemios /work/phemios --output /tmp/audio-acceptance --reference
cargo metadata --offline --format-version 1 --manifest-path /tmp/audio-acceptance/Cargo.toml > /tmp/audio-acceptance/metadata.json
NATIVE_APE_FIXTURES=/work/aede/crates/aede-audio/tests/fixtures/native-extra NATIVE_APE_REFERENCE=/tmp/audio-acceptance/reference/sh3.ape python3 tools/native-platform/run.py --aede /work/aede --flaccompagnon /work/FlacCompagnon --phemios /work/phemios --acceptance /tmp/audio-acceptance --target aarch64-apple-darwin --offline
```

Only the explicit `--reference` preparation downloads the hash-pinned historical recording. Existing wrong-hash bytes are refused, never silently replaced. An existing acceptance lock is retained; dependency resolution is explicit before the locked runner. `NATIVE_AUDIO_CAPTURE_DIR` optionally saves the renderer's unchanged PNGs for local visual inspection.
