# Pinned ALSA-rs source

This directory contains the published `alsa` crate version 0.11.0, originally
identified by crates.io checksum
`812947049edcd670a82cd5c73c3661d2e58468577ba8489de58e1a73c04cbd5d`.
Upstream: <https://github.com/diwic/alsa-rs>. The original Apache-2.0 and MIT
license files are retained. Registry bookkeeping and the upstream crate's
standalone lockfile are omitted.

The sole source patch adds three safe, read-only getters to `src/pcm.rs`:

- `PCM::is_hardware()` queries the actual opened PCM plugin type.
- `HwParams::get_sbits()` returns effective sample-resolution bits, preserving
  negative ALSA errors instead of interpreting container width as precision.
- `HwParams::get_rate_numden()` returns the exact configured rate ratio,
  preserving ALSA errors and refusing a zero denominator.

The calls remain inside ALSA-rs's existing owned-handle and FFI boundary. No raw
pointer escapes and no memory-layout reinterpretation is used. These getters
do not select a device, write mixer controls, or certify digital device output.

The workspace patch affects CPAL's existing ALSA dependency and the CLI's direct
Linux GNU dependency without changing the package version or adding another
ALSA dependency tree. The vendored crate is excluded from workspace members;
its upstream tests and implementation are otherwise retained, including their
upstream layout. Reconcile this additive patch and its safety review when
updating the pinned upstream crate rather than editing registry-cache files.
