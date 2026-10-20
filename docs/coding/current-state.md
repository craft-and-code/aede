# Aède — Current state

## Project status

Aède 0.6.1 is the local-first music engine and CLI, with a separate Tauri server App. FlacCompagnon core is pinned to published v0.9.6. The [roadmap](../design/roadmap.md) remains authoritative: M0 catalogue/storage and M1 enrichment are implemented; M2 API/persistence is implemented with deployment capacity acceptance pending; M3 playback is software-hardened with physical/platform acceptance pending; M4 has initial network-device profiles.

The catalogue is graph-based and common to installation accounts. Personal annotations, ratings, favourites, tags, collections, histories, preferences, checkpoints and achievements remain owner-private. Accounts use stable identity, role/session checks and independent protected storage. Parental policies retain exclusive supervisor locks, civil-time windows and bounded listening leases. Guest use remains unrestricted by design. See [accounts](../design/accounts.md), [isolation](account-isolation.md) and [parental controls](../server/parental.md).

## Delivered interfaces

- The CLI scans/browses/queries the local graph, preserves deterministic identities and attributed source claims, checks integrity, imports analyses and explicitly generates sidecars/exports. Source audio/tags are never changed.
- The HTTP/JSON/WebSocket server provides bounded catalogue reads, private routes, task activity and authenticated PCM. Jobs retain completed work and can survive client disconnection; process-local task registries do not imply restart persistence. See [API](../api.md), [route inventory](../../crates/aede-server/README.md) and [jobs](../server/jobs.md).
- Administrative MusicBrainz review exposes the existing durable accept/reject/undo rules with optimistic revisions and explicit exact-artist biography tasks; see [source review](../server/source-review.md). Pending native-client rebuild/live-provider acceptance remains distinct from fixture verification.
- Native presentation supports source-qualified artist/album/track/label/composer navigation, public cache projections, private dynamic browsing collections, missing-discography suggestions, analysis/report reconstruction, lyrics, waveforms and original-file exports. Capability names distinguish old running engines from confirmed empty data. See [presentation](../server/presentation.md).
- The [server App](../desktop.md) uses isolated application data, local HTTPS, editable roots/scans, public connection-profile export and bounded sanitized status/log views. CLI data is separate. [Parent-owned embedding](../server/desktop.md) supports Phémios over ephemeral HTTPS/WSS with process-only certificate trust.
- Local playback supports without-effects, DSP and a narrowly admitted direct-ALSA strict bit-perfect route. Native remote PCM uses client consumed-frame ACKs; local history has a different submitted/estimated timing boundary. See [playback](../design/playback.md), [DSP](dsp-review.md) and [acceptance](bit-perfect-acceptance.md).
- UPnP, OpenHome, Cast and SlimProto reuse `aede-devices`. Authenticated graphical profiles use exact-login ownership, a finite queue and heartbeat leases; physical interoperability remains separate. See [devices](../server/devices.md).

## Verification status

The 2026-10-20 workspace verification passed 2,692 Rust tests (unit, integration and documentation tests), including M1 provider-identity receipts, MusicBrainz redirects, retained review decisions and shared recording-credit diagnostics, alongside the retained M2–M4 shutdown, authorization and cancellation regressions, with required FFmpeg coverage. Helper/site/release/terminal checks, Clippy, Rustdoc and the optimized CLI build also passed. This is software evidence; it does not establish live-provider, native-client or physical-output acceptance.

The root gate is `tools/check.sh`; the independent desktop gate is `desktop/tools/check.sh`. The root gate completed in full; the unchanged desktop App was not rebuilt. [CI/release procedures](ci-and-releases.md) describe artifacts and versioning; [desktop dependencies](desktop-dependencies.md) records exact approved GTK warnings. Fresh advisory lookups are separate from offline checks and do not cover pinned Git sources. Release CI and artifacts remain the authority for a tagged platform build.

## Remaining acceptance and limits

Physical gapless/digital bit-perfect output, Windows console/device behavior, strict ALSA runtime, target-NAS memory/throughput/concurrent-client capacity and real LAN renderers require their own environments. Synthetic signals, metadata harnesses and software loopbacks do not certify those targets. Docker/service deployment, richer device registries and additional queue/session designs remain roadmap work.

No metadata-only browse initiates enrichment. Public caches/roots stay common; the trusted OS operator and installation token are outside account privacy. JSON stores, shared writer budgets, filesystem TOCTOU constraints and failure-preserving publication remain explicit. SQLite is a measured option, not an M2 requirement.

## Engineering map

- [M0 catalogue/storage](m0-review.md), [M1 source/enrichment](m1-review.md), [M2 server/publication](m2-server-review.md): durable invariants and boundaries.
- [M3 transport](m3-review.md), [DSP processing and measurements](dsp-review.md), [M4 ownership](m4-review.md): audio-specific contracts.
- [Engineering rules](engineering-rules.md), [storage benchmark](m2-storage-benchmark.md), [gapless protocol](gapless-measurement.md): procedures and qualified evidence.
- `CLAUDE.md`: permanent development instructions; `docs/design/`: design decisions; the bilingual site/manual explains operation.

Update this page when delivered capability or acceptance changes. Keep regression mechanics in tests and detailed decisions in their domain document, rather than appending per-pass status reports.
