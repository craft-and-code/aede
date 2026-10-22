# AE-STAB-001 — Audit Scope

## Campaign information and authorization

- Project: Aède.
- Campaign ID: `AE-STAB-001`.
- Campaign type: `STABILIZATION` — first general campaign under this quality process; earlier milestone reviews remain historical evidence.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- Initial code-review target: the starting commit. Record any later candidate revision and relevant dirty state before reviewing it.
- Preparation date: `2026-10-22`.
- Responsible authority: Aède maintainer; decision attribution remains to be recorded without personal identifiers.
- Target release/version and final candidate: not selected. Campaign closure does not select or approve a release.
- Scope proposal: approved by the maintainer; prepared documentation awaits validation.
- Current phase: documentation preparation. First code audit: `NOT STARTED`.

Only necessary Markdown changes within `docs/quality/` and read-only documentary consistency checks are authorized now. Do not begin the code audit, execute tests/builds/lints/security checks, modify other files or create commits. Stop for maintainer validation of these documents before the first audit. The listed later phases and commands do not grant permission to execute them.

## Objectives and invariants

Inspect all Aède components at an agreed general-stabilization depth, following important execution paths and deepening the review where evidence indicates risk. Record actual coverage; this is not a claim of exhaustive inspection of every line or interleaving.

- Prioritize data loss/corruption, disclosure, authorization failures, crashes, resource exhaustion and silently incorrect results.
- Assess recent shared-audio extraction and cross-component lifecycle/publication boundaries.
- Preserve local-first behavior, unchanged original audio/tags, deterministic graph identities and explicit network access.
- Preserve provenance and distinctions between local facts, external claims and derived conclusions.
- Preserve compatible persisted data, owner-private state, safe publication and explicit refusal of unsupported CLI/API requests.
- Identify behavioral regression needs and residual risks without treating missing tests as demonstrated defects.

## Included components

| Paths | Review focus |
| --- | --- |
| `crates/aede-core/` | Native/fallback readers, malformed input, scan/root retention, graph identity, queries, JSON stores/conclusions, migrations, writer locking, backup/restore, copying/conversion and exports |
| `crates/aede-accounts/` | Stable owners, roles, credential/client-key policy and account lifecycle |
| `crates/aede-server/` | HTTP/JSON/WebSocket and Subsonic contracts, HTTPS, role/owner checks, revocation, parental policies, jobs, cancellation, bounded work, PCM/ACKs and final publication admission |
| `crates/aede-moods/` and relevant core source/analysis paths | Owner-scoped opinions, derived caches, identity ambiguity, source review/reconciliation, provenance and explicit enrichment |
| `crates/aede-cli/` | Parsing/help/dispatch agreement, strict arguments, human and machine output, terminal escaping, destructive confirmation and orchestration |
| `crates/aede-audio/`, `crates/aede-dsp/` and core/CLI audio integration | Feature isolation, progressive decoding, complete frames, bounded queues, source identity, numerical safety, seek/EOF, shutdown and consumption/history boundaries |
| `crates/aede-devices/` | Explicit discovery, original-file serving, protocol admission, certificate/peer identity, exact-login ownership, leases, takeover and cleanup |
| `desktop/` | Independent Rust/SolidJS/Tauri workspace, protected private profile, TLS identity, child-process lifecycle, IPC permissions, logs and CLI/App data separation |
| `vendor/alsa-0.11.0/` local patch and its authored callers | Local changes and strict-output evidence contract; no exhaustive upstream ALSA audit |
| `tools/`, `.github/workflows/`, workspace manifests/lockfiles and relevant `docs/`/`site/` interfaces | Pinned dependencies, verification coverage, versions, artifact contents, documentation contracts and generated-site checks |

Follow the [documentation map](README.md#purpose-and-document-map), [architecture](../design/architecture.md), [roadmap](../design/roadmap.md), [shared-audio design](../design/shared-audio.md), [account isolation](../coding/account-isolation.md) and [desktop guide](../desktop.md). Read the domain documents relevant to each execution path before reviewing it.

## Exclusions and separate acceptance

- Full source audits of Phémios and FlacCompagnon are excluded: they are separate repositories. Aède's integration contracts and pinned dependency boundaries remain included.
- Exhaustive audits of third-party and vendored code are excluded. Dependency advisories and the local ALSA patch remain included.
- Future functionality, architectural rewrites, dependency upgrades and storage replacement are excluded unless separately scoped around a confirmed defect.
- Full website editorial/design review is excluded; existing rendering, bilingual-link and release checks remain included.
- Physical audio, real LAN renderers, live providers/clients and target-NAS deployment are separate acceptance activities. Their unavailable environments must be recorded, never described as audited or passed.

## Priority interactions and recent changes

The following are documented review priorities, not newly confirmed defects:

- Store replacement/new-file publication, permission preservation, migration/restore failure and interruption under concurrent writers.
- Authentication or parental-policy changes during queued preparation, followed by catalogue/private/media publication; exact captured-login checks and cleanup after shutdown/revocation.
- Hostile/truncated binary, JSON, playlist, path and external-service input; bounded CPU/memory, subprocess output and worker admission.
- Shared public caches versus private projections, including moods, missing-album decisions, histories and player checkpoints.
- Shared-audio extraction at `d8c0f1a8` and subsequent source-selection documentation: preserve queue/format behavior and isolation of decode, DSP, output and loudness features.
- Cross-milestone cancellation/source-identity work recorded in [current state](../coding/current-state.md) and the M0–M4 reviews. Recheck relevant paths rather than importing old conclusions as current verification.
- The documented published `v0.6.1` predates the shared crates. The next stable release must contain them for automatic downstream preparation; local library checks do not establish downstream native-App acceptance.

No new incident, format migration or API defect is asserted by this preparation. Establish actual changes and triggering scenarios during the authorized audit.

## Validation controls and evidence

The [root gate](../../tools/check.sh), [desktop gate](../../desktop/tools/check.sh), [CI/release procedures](../coding/ci-and-releases.md), [CI workflow](../../.github/workflows/ci.yml), [security workflow](../../.github/workflows/security.yml), [desktop scripts](../../desktop/package.json) and shared-audio design establish these controls. Commands below run from the repository root unless stated otherwise. Record actual results only in the [checklist](release-checklist.md#3-validation-results), using these control IDs.

### Mandatory software controls for campaign closure

| ID | Control | Command / required evidence |
| --- | --- | --- |
| AE-C01 | Complete root gate with FFmpeg required | `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh` |
| AE-C02 | All root workspace features: unit, integration and documentation tests | `cargo test --workspace --all-features --locked --offline` |
| AE-C03 | Shared audio with no default features | `cargo test -p aede-audio --no-default-features --locked --offline` |
| AE-C04 | Decode-only shared audio | `cargo test -p aede-audio --no-default-features --features decode --locked --offline` |
| AE-C05 | DSP without loudness | `cargo test -p aede-dsp --no-default-features --locked --offline` |
| AE-C06 | Full-feature static analysis | `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` |
| AE-C07 | Minimal-feature static analysis | Run the three feature-isolation lint commands below |
| AE-C08 | Independent desktop gate | `bash desktop/tools/check.sh` |
| AE-C09 | Native platform and minimum-version software evidence | Native engine CI on macOS/Linux/Windows at Rust 1.89 and desktop CI on those platforms; identify candidate SHA, toolchain, jobs and results. Windows uses `cargo test --locked` and helper commands from the CI workflow with FFmpeg required. Desktop Rust baseline is 1.90; stable-toolchain success alone does not establish that minimum. |
| AE-C10 | Offline advisory-client regressions | `python3 -B -m unittest discover -s tools -p audit_dependencies_tests.py` |
| AE-C11 | Root registry advisory lookup | `python3 -B tools/audit-dependencies.py` |
| AE-C12 | Desktop registry advisory lookup under existing policy | `python3 -B tools/audit-dependencies.py --manifest-path desktop/Cargo.toml --desktop-compatibility-warnings` |
| AE-C13 | Desktop npm advisory lookup | `npm audit --prefix desktop` |

AE-C07 commands:

```sh
cargo clippy -p aede-audio --no-default-features --all-targets --locked --offline -- -D warnings
cargo clippy -p aede-audio --no-default-features --features decode --all-targets --locked --offline -- -D warnings
cargo clippy -p aede-dsp --no-default-features --all-targets --locked --offline -- -D warnings
```

AE-C02 includes the documented `aede-audio --all-features` test profile. Run individual tests first when needed; a targeted pass does not replace the final mandatory controls. Record skips/ignored cases and required real encoded fixtures rather than assuming feature coverage from a count.

AE-C01 includes `cargo fmt -- --check`, `cargo clippy --locked --offline --all-targets -- -D warnings`, `cargo test --locked --offline`, `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --no-deps --quiet` and `cargo build --locked --offline --release`. It also covers the Python helpers, release coherence, masked-entry pseudo-terminals, generated statistics and bilingual site/Rustdoc publication checks. Evidence for the aggregate gate may cover these constituent checks; do not invent separate runs.

AE-C08 includes frontend formatting/lint/tests, TypeScript/Vite build, TypeScript documentation, component checks, Tauri version alignment, exact-engine preparation and Rust format/test/Clippy/Rustdoc/build. Its Rust tests enable `aede-desktop-core/engine-tests`. A desktop gate pass is not a packaged native-App acceptance result.

Applicable controls may be satisfied by accessible CI evidence for the exact candidate and configuration. Record each AE-C09 platform/toolchain subresult; a partial matrix is not an aggregate `PASS`. Root and desktop dependencies, native SDKs, Linux ALSA/GTK prerequisites, local sockets, pseudo-terminals and FFmpeg must be available. Locked fetch/install preparation is a separate authorized activity, not a successful test. Do not use `tools/build.sh` or dependency-update helpers to change the reviewed baseline during validation.

AE-C11–AE-C13 require explicit network access when execution is authorized. Registry lookup sends package names/versions and excludes pinned Git sources and external tools. Preserve visible policy-approved warnings; do not expand exceptions. [Desktop dependency policy](../coding/desktop-dependencies.md) records the exact existing GTK exceptions. Fresh lookups are required; old advisory or CI results do not validate a later candidate.

### Supplementary acceptance and release-only controls

These are separate from the mandatory software controls above. Unavailable acceptance remains a documented campaign limit and may still block a release or deployment claim.

| ID | Activity | Procedure / applicability |
| --- | --- | --- |
| AE-A01 | Windows interactive console and native output | [Console acceptance](../coding/ci-and-releases.md#windows-console-playback-acceptance); requires an interactive Windows host/device |
| AE-A02 | Physical continuity, strict ALSA runtime and digital bit-perfect output | [Gapless protocol](../coding/gapless-measurement.md) and [strict acceptance](../coding/bit-perfect-acceptance.md); requires Linux hardware and suitable captures |
| AE-A03 | Real network renderers, providers and clients | Relevant protocol/provider/client procedures; record firmware/client versions and exact accepted operations |
| AE-A04 | Target-NAS capacity, service lifecycle and recovery | [Storage benchmark](../coding/m2-storage-benchmark.md) and [operating guide](../operating.md); agree real-library size, RAM/CPU, concurrency and latency budgets first. `bash tools/benchmark-catalog.sh` supplies synthetic evidence only. |
| AE-R01 | Candidate packaging and publication readiness | [Release workflow](../../.github/workflows/release.yml): target tests/builds, CLI/App archives and checksums; `npm run build:native` from `desktop/` builds a host package. Requires a selected candidate and separate release decision. |

## Verification limits and documentary follow-up

- Preparation identified a macOS ARM64 host with Rust/Cargo 1.99, Rustfmt/Clippy, Node 26, npm, Python, FFmpeg and ffplay. Tool discovery/version output is not compilation or test evidence. `rustup` was not in `PATH`; Rust 1.89/1.90 minimum-version verification remains outstanding.
- Dependency-cache availability, native build prerequisites, socket and pseudo-terminal permissions remain unverified by execution. Network restrictions may block fresh advisory lookup or dependency preparation; that would block the relevant mandatory control.
- macOS checks cannot establish native Windows behavior, Linux ALSA runtime, physical bit-perfect/gapless output, renderer interoperability or NAS capacity. ACKs/software loopbacks do not prove audible output.
- Shared JSON stores, resource budgets and filesystem TOCTOU limits do not provide whole-process memory bounds, fairness or protection from an equally privileged hostile OS process. The OS operator and installation token remain trusted; catalogue roots/caches are shared installation data.
- Historical verification is linked from current state and is not a pass for this campaign. No test count is copied or updated here.
- Known documentation discrepancies outside the current editable scope: the architecture map omits audio/moods crates; its workflow/site description is outdated; the roadmap's future device-API wording needs reconciliation with delivered authenticated profiles. These are documentary follow-ups, not newly audited code defects, and those documents remain unchanged.
- Documentary link checks cover local targets and linked anchors within `docs/quality/`; remote URLs and anchors in external documents remain unvalidated.

No genuine `AE-*` finding, rejection or deferral has been recorded yet; see the [register](findings.md). An empty register does not establish the absence of defects.

## Review, closure and current outcome

The [README closure policy](README.md#campaign-closure-policy) is authoritative: no unresolved Critical, no unresolved High for normal closure without an exceptional documented maintainer waiver, qualified/resolved or justified deferred Medium/Low, all mandatory controls passing and two consecutive independent reviews with no new confirmed Critical/High/Medium defect. Record unavailable supplementary acceptance explicitly. Closing this campaign is separate from authorizing a release.

- Campaign verdict: `CONTINUE`.
- First audit and independent reviews: `NOT STARTED`.
- Findings created, fixed, rejected or deferred: none recorded.
- Software validation: all controls `NOT RUN`; no application tests, builds, lints or security lookups executed during preparation.
- Documentary checks: file/command-source inspection, local-link and Git-diff consistency checks only; these do not satisfy any software validation control.
- Next action: maintainer validation of the prepared documentation before the first read-only audit.
- Remaining decisions: accountable approval attribution, future final candidate/release targets, review assignments, and hardware/client/NAS acceptance environments and measurable budgets. Evidence will be retained in the register and checklist; no separate report file is required.
