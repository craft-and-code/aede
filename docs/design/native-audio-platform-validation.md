# Local native audio qualification

## Scope

These checks qualify sources without a Tauri App, product frontend bundle, embedded engine, installer, release or remote runner. The three consumers are Aède file decoding, FlacCompagnon preview/PCM/spectral visits and Phémios CUE/APE. They reuse one `aede-audio` implementation; there are not three copies of the SDK. The immutable dependency graph is recorded in the manifests/lockfiles and [shared-source integration](shared-audio.md#aligned-development-source-graph).

## 2026-10-28 local evidence

| Target | Production native C/C++ build | SDK/bridge link probe | Executed decoding |
| --- | --- | --- | --- |
| macOS ARM64 | Passed | Passed through Rust consumers | Shared native tests and five three-consumer acceptance cases passed |
| Linux GNU x64 | Passed | Passed | Not executed |
| Linux GNU ARM64 | Passed | Passed | Not executed |
| Linux musl x64 | Passed | Passed, static ELF | Not executed |
| Linux musl ARM64 | Passed | Passed, static ELF | Not executed |
| Windows GNU x64 | Passed | Passed, PE x64 | Not executed |
| Windows GNU ARM64 | Passed | Passed, PE ARM64 | Not executed |
| Windows MSVC | Not run | Not run | Not executed |

Cross checks used the official Zig 0.17.0 macOS ARM64 compiler, the actual Cargo-generated `aede-audio` build-script executable, and LLVM object-header inspection. Each invocation set Cargo's target OS/architecture/environment and target-specific CC/CXX/AR wrappers for the corresponding Zig triple. The wrappers removed cc-rs's redundant Rust `--target` argument before selecting the Zig triple. The full production build script compiled WavPack, Musepack, Speex and the SDK/bridge. Every SDK archive member had the expected architecture. `tools/native-platform/link_probe.cpp` was linked with that archive, C++11 and pthread support; the musl ELF probes had neither an interpreter nor shared-library dependencies. No target executable was run on the Mac host. This is not a complete cross-compiled Rust application or MSVC test.

Reproduced failures were missing Linux C11 declarations for WavPack's `fileno`, `strdup` and `strcasecmp`, missing Speex generated fixed-width type declarations, and WavPack's Windows GNU selection of undeclared `_BitScanReverse`. The build now enables the appropriate POSIX/BSD declarations only for Linux WavPack, supplies an adapter-owned stdint Speex type header and selects the existing zero-guarded `clz` builtin with GNU/Clang. The same six builds/links passed after correction. No SDK/vendor algorithms or redistribution notices were changed.

The native macOS acceptance suite exercises the actual public APIs of all three consumers with old 3.95/3.97 and modern 3.99 APE, original sample comparisons, complete frame counts, forward/backward/frame-edge/EOF seeks, CRC refusal before publication and unchanged source bytes. A child process has an empty PATH. The pinned real nonzero `sh3.ape` 3.97 reference is checked against independent PCM MD5 before every-sample/seek comparisons; Phémios correctly refuses its nonintegral CD-sector duration. The recording is temporary, not redistributed. Both shared Rust PNG geometries are checked and their unchanged outputs are previewed through Phémios's real result panel.

## Repeatable local runtime checks

`tools/native-platform/prepare.py` creates a disposable test-only Cargo workspace over explicit source paths. Freeze its lock with Cargo metadata, then use `run.py --offline` with the native Rust target, shared fixture directory and verified historical reference. The [tool instructions](../../tools/native-platform/README.md) describe inputs and provenance. Reports record compiler, source commits and dirty state, frozen-lock hash and executable architecture. They do not identify an unrelated dirty working tree as a clean immutable release.

Run each repository's `bash tools/check.sh --sources-only` after dependency alignment. Native Windows/Linux execution, MSVC, physical output and installed-App delivery remain acceptance work requiring suitable local machines or an explicitly authorized later remote run. A GNU link probe cannot certify MSVC, runtime behavior, throughput, filesystem policy or a device. Normal stable-source selection/publication remains separate.
