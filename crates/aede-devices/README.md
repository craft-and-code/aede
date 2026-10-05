# Aède devices

Explicit LAN discovery and finite original-audio playback for SlimProto,
UPnP AVTransport and OpenHome Playlist. The [device guide](../../docs/server/devices.md)
defines the supported profiles, security boundaries and community trials.
Real-device acceptance remains pending; the extraction does not add protocols.

## Responsibility and dependencies

- `discover(bind)` performs one bounded SSDP discovery on the chosen interface.
- `cast(options, paths)` supplies 1–64 ordered occurrences to one selected LAN
  player. Duplicate paths remain duplicate occurrences; original bytes are
  transferred unchanged and the player owns decoding and output.
- `CastOptions::validate` rejects unsupported options/addresses before file
  inspection or network listeners are opened.
- `original` provides the shared MIME/suffix and single HTTP byte-range policy
  used by this crate and the Subsonic adapter. It contains no routes or account
  policy; each caller maps errors and implements its own HEAD behavior.

The crate uses `aede-core` for tag inspection, format properties and secure
session tokens, plus the already approved Tokio, Serde and Hyper stack. It
does not depend on `aede-server` or add any third-party package to the lockfile.
The CLI calls it directly. `aede_server::devices` remains a compatibility
reexport; there is only one implementation of each protocol and media policy.

Catalog selection, account authorization, persistent personal data and decoded
DSP remain in their existing layers. Network transfer or an unauthenticated
player status never writes listening history. Subsonic/OpenSubsonic remains in
`aede-server`, serving authenticated software clients rather than controlling
LAN renderers.

## Operation and limits

The entry points are blocking and own a bounded Tokio runtime. `cast` monitors
Ctrl-C, prints listener addresses, requests device Stop and closes its temporary
media listener when finished. An async API integration must provide a deliberate
worker/lifecycle/cancellation design rather than call them on a Tokio worker.
This first extraction retains the existing Terminal behavior.

Only specific private, link-local or loopback IPv4 addresses are accepted.
Selected originals are read-only and checked before/opening/during transfers;
changed paths or link replacements stop playback. A fresh capability and exact
peer-IP admission protect the temporary selection-only HTTP listener. This
requires a trusted LAN and does not provide encrypted transport or authenticated
device identity.

No normalization, EQ, transcoding, listening-history write or background
discovery is performed. The initial profiles retain their existing finite queue,
format preflight, parser/body/memory limits and shutdown budgets. See the guide
for unsupported controls and hardware/gapless/multiroom acceptance.

## Tests

Unit tests live in sibling `*_tests.rs` files, with media fixtures in a dedicated
test-only support module. The original controller regressions and shared-media
policy tests are moved once from `aede-server`, preserving their behavior and
count. They exercise real loopback sockets, scripted players, malformed protocol
messages, original-byte transfers, replacement/refusal and cancellation bounds.

```sh
cargo test --locked --offline -p aede-devices
bash tools/check.sh
```
