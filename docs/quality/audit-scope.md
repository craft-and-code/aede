# AE-STAB-001 — Audit Scope

## Campaign information and authorization

- Project: Aède.
- Campaign ID: `AE-STAB-001`.
- Campaign type: `STABILIZATION` — first general campaign under this quality process; earlier milestone reviews remain historical evidence.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- First core audit target: `ebba04dac6757fe57d3d546367c81e59b5c0f033`; CLI pass baseline: `560fb8675cb7b46635dbdcc44aab8caa44edb38a`. Record later candidate revisions and relevant dirty state before reviewing them.
- Preparation date: `2026-10-22`.
- Responsible authority: Aède maintainer; scope, phase and local-commit authorization recorded from the 2026-10-22 maintainer instructions, without personal identifiers.
- Target release/version and release candidate: not selected. CLI-C3 is a reviewed software-fix candidate; campaign closure does not select or approve a release.
- Scope and prepared documentation: approved by the maintainer.
- Current phase: AE-014/AE-015 accepted after independent R-ACC-01 on frozen ACCOUNTS-C1, based on `3919d0422148e5daecd2d44c3728d27fcf04082b`. The maintainer authorized recording the review/limits, the complete root gate and local commits after final diff verification. Complete AE-C01 PASS and final checks preceded the two local source/test commits; the combined software tree is `d97d45efb4c41ec5ced85dc64f61ddcd98fa52f9`, identical to the reviewed nine-file fingerprint. Quality records are packaged separately. No push, tag or release is authorized.

On 2026-10-22 the maintainer authorized the CLI orchestration proposed in the conversation: fresh-context auditor, test engineer, targeted corrector, two consecutive independent adversarial reviewers and final validator; quality-document updates and local commits after validation. Only one agent writes at a time. Auditors/reviewers remain read-only; no dependency upgrades, push, tagging or publication is authorized. Earlier phase restrictions describe historical permissions and have been superseded by these explicit instructions.

The subsequent AE-009 instructions authorized the consent implementation for `import --forget` and `sources --forget`, regressions, script migration and finding updates, initially without a commit, then a fresh-context independent review and regression verification. The maintainer later accepted AE-009 and authorized recording R-AE009-01 with its limits and creating local commits after final diff verification. Scope included the necessary server forwarding flush; other `--forget` meanings were unchanged. Software was committed as `e4df4f10ed6d60d1adfcdb146306958f8aa096a7`, with quality records in `5c2fe27f3bdef152c6921ecb8cf8a559f33d0a4d`, without push. Those packaging permissions are historical; subsequent explicit server/accounts instructions supersede the old prohibition on a next audit and do not carry forward commit authorization.

### Current focused pass: aede-accounts

The maintainer authorized a read-only accounts pass with fresh-context agents, then accepted its two Low/HIGH findings before any correction on 2026-10-22. Auditors `accounts_domain_auditor` and `accounts_storage_auditor` inspected the domain and persistence/integration partitions; `accounts_test_engineer` independently examined coverage and executed existing tests and isolated diagnostic probes. These discovery roles are not post-fix independent reviewers.

Actual inspection covered all account-domain implementation/tests, the core protected-file adapter and backup compatibility, CLI account/key administration and restore, and relevant server session/key-cache revalidation. Account identities, roles, last-administrator retention, generations, secret disclosure and data preservation were followed across these boundaries. This is not a general review of every server or personal route.

Correction scope is AE-014 (panic-free stored-verifier parameter validation and actual Base64 salt decoding for passwords and client keys) and AE-015 (account construction/mutation timestamps must fit the reader's range, with unchanged state on refusal). Existing valid credential formats, ownership, hash policy and unrelated behavior must remain compatible. No dependency/format change or unrelated cleanup is authorized. Fresh-context correctors own separate tests and take sequential ownership of the shared `lib.rs`; a separate Test Engineer owns adapter regressions. The parent coordinates before/after execution, integration and quality records.

Read-only baseline: 50 targeted tests PASS. Diagnostic probes reproduced both findings outside the repository; the parallelism panic was observed in debug, while an executed standard release probe rejected it without a panic. The complete root gate was not run during the audit/correction stage; the subsequent final AE-C01 PASS on unchanged ACCOUNTS-C1 is recorded in the checklist. The separate platform/all-feature matrix remains unexecuted. Record actual regression failures before production changes, then rerun the domain, adapter and relevant CLI/server tests and targeted lint/format/document checks. Historical SERVER-C2 gate evidence does not validate this later software.

Limits include native Windows/Linux/MSRV, exhaustive failure injection/interleavings, live CLI restoration against existing server sessions and cached keys, and broader client/provider/NAS acceptance. Core validation already prevented invalid timestamp states from replacing credentials; no timestamp-triggered persisted corruption or present-day request-controlled clock path was established. R-ACC-01 independently supports both corrections without a new confirmed defect. It is one affected-subset review; durable auxiliary-layout authentication and live restore/session/cache coverage remain strengthening follow-ups. The maintainer accepted the result and authorized review/limit recording, complete gate and local commits after final diff checks. Release remains separate.

### Earlier focused pass: aede-server

The maintainer authorized a first read-only server pass with fresh-context agents, then accepted its four confirmed defects on 2026-10-22 before any correction. Baseline: clean `main` at `5c2fe27f3bdef152c6921ecb8cf8a559f33d0a4d`. Two auditors independently inspected access/publication and administrative/media lifecycle partitions; a separate Test Engineer investigated existing coverage and reproduced failures in an isolated temporary mirror. These roles are not post-fix independent reviewers.

Actual inspection covered HTTP/JSON/WebSocket and Subsonic boundaries, account/session and parental revalidation, personal preferences/moods and other private routes, jobs and synchronous administrative scans, CLI delegation/cancellation, catalog/conclusions reload, shutdown, catalog/inspection/presentation/file exports, PCM/history and network-player paths at the depth recorded in the register/checklist. Follow other crates only where necessary for these contracts; this is not exhaustive inspection of every route, file or interleaving.

Server correction scope was AE-010 (notification policy revalidation), AE-011 (generation exclusion across the historical bodyless scan route), AE-012 (moods write query refusal) and AE-013 (duplicate preference keys), with regression and adjacent-behavior checks. Three fresh-context correction agents owned disjoint notification, administrative-job and personal-input files; the parent owned integration and quality records. Existing coverage and data were preserved. No dependency/format change or unrelated cleanup was authorized. Initial correction and review phases prohibited commits; subsequent maintainer acceptance authorized local commits after the complete root gate and final diff verification. These historical permissions do not authorize accounts commits. The combined server software was committed as `78e0f01c2f1cf9016a8366e75bd237eb498c68b9`; its records are in `3919d0422148e5daecd2d44c3728d27fcf04082b`. No push or release was authorized.

Remaining scope limits include native Windows/Linux/MSRV, real providers/clients/renderers, physical audio, target NAS, exhaustive write-failure/interleaving tests and a transport-level reproduction of the internal cancelled-scan observation. Direct handler cancellation alone does not establish that a real disconnected client loses a terminal event. R-SRV-01 independently supports all four corrections without a new confirmed defect. It is one affected-subset review, not a general-campaign pair or closure. Its snapshot-rendezvous and blocked-publication assertion gaps remain explicit test-strengthening follow-ups.

### Earlier focused pass: aede-cli

Inspect argument/option/help/dispatch agreement, refusal of unsupported inputs, errors and exit status, stdout/stderr and machine formats, terminal escaping, destructive confirmation, paths, import/export/persistence, delegated work and cancellation. Follow other crates only where needed to establish these execution paths. The CLI baseline was clean on `main` at the revision above. AE-001–AE-003 remain reserved for the previous findings.

Run focused reproductions before corrections where practical, preserve assertions/coverage, then execute the full root gate before local commits. Freeze code/tests during the two independent reviews; a new confirmed significant defect resets their clean sequence. These are focused CLI reviews, not whole-campaign closure reviews. The general campaign controls below remain mandatory for eventual closure; unexecuted unrelated profiles/platforms are not silently waived by this narrower pass.

The targeted corrections include minimal English/French command-reference amendments for the changed public option/mode contract. They do not authorize unrelated manual cleanup. Final software candidate CLI-C3 and its reproducible source/test fingerprint are recorded in the register; review/validation documentary bookkeeping does not change that frozen software candidate. R-CLI-02 discovered significant AE-008 on C2, resetting the earlier sequence; R-CLI-03 and R-CLI-04 independently reviewed C3 consecutively without a new significant defect.

AE-009's later candidate AE009-C1 is identified separately by its base and expanded 200-file source/test fingerprint in the register. Its policy, default/minimal regressions, root gate and R-AE009-01 are recorded in the checklist/register. The previous clean pair applies to CLI-C3; R-AE009-01 is one independent review of the affected CLI/server delta and does not establish a second current-candidate or general-scope review. Original Low policy ambiguity and its resolution remain traceable. Durable test-assertion/harness gaps, absent populated accounts/parental fixtures and unexecuted mixed-version acceptance remain explicit limits, not new confirmed product defects.

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
- CLI-C3's root gate verified the cached root dependencies and required local socket, pseudo-terminal and process-inspection prerequisites on this host. Independent desktop/native SDK and other-platform prerequisites remain unverified. Network restrictions may block fresh advisory lookup or dependency preparation; that would block the relevant mandatory control, not establish a pass.
- macOS checks cannot establish native Windows behavior, Linux ALSA runtime, physical bit-perfect/gapless output, renderer interoperability or NAS capacity. ACKs/software loopbacks do not prove audible output.
- Shared JSON stores, resource budgets and filesystem TOCTOU limits do not provide whole-process memory bounds, fairness or protection from an equally privileged hostile OS process. The OS operator and installation token remain trusted; catalogue roots/caches are shared installation data.
- Historical verification is linked from current state and is not a pass for this campaign. No test count is copied or updated here.
- Known documentation discrepancies outside the current editable scope: the architecture map omits audio/moods crates; its workflow/site description is outdated; the roadmap's future device-API wording needs reconciliation with delivered authenticated profiles. These are documentary follow-ups, not newly audited code defects, and those documents remain unchanged.
- Documentary link checks cover local targets and linked anchors within `docs/quality/`; remote URLs and anchors in external documents remain unvalidated.

The [register](findings.md) retains the core findings, fix commits and subsequent CLI evidence. Historical execution records are identified separately from fresh validation.

## Review, closure and current outcome

The [README closure policy](README.md#campaign-closure-policy) is authoritative: no unresolved Critical, no unresolved High for normal closure without an exceptional documented maintainer waiver, qualified/resolved or justified deferred Medium/Low, all mandatory controls passing and two consecutive independent reviews with no new confirmed Critical/High/Medium defect. Record unavailable supplementary acceptance explicitly. Closing this campaign is separate from authorizing a release.

- Campaign verdict: `CONTINUE`.
- Current accounts pass: AE-014/AE-015 accepted and corrected on ACCOUNTS-C1, uncommitted when reviewed and now committed locally as `d97d45efb4c41ec5ced85dc64f61ddcd98fa52f9`; fingerprint, per-finding mapping and exact evidence are in the register/checklist. Domain tests PASS in debug/release, adapter/backup and selected CLI/server neighbors PASS (91 executions total), targeted all-target Clippy PASS. Sixteen regression/control tests were added without removing existing coverage. R-ACC-01 supports both accepted corrections, with its two test-depth limits retained; one review does not establish an accounts/general-campaign pair.
- Core persistence and earlier CLI passes: completed within their recorded limited scopes; AE-001–AE-009 verified. Server AE-010–AE-013 are corrected, behaviorally verified and accepted on SERVER-C2, with intermediate failures retained. R-SRV-01 is one independent affected-subset review without a new confirmed defect; earlier core/CLI reviews do not establish a current server or general-campaign pair.
- Software validation: complete AE-C01 PASS on unchanged ACCOUNTS-C1 (2,780 Rust tests, none failed/ignored, 15 masked-entry terminal tests, helper/site checks, Rustdoc and optimized build). Parent execution followed accepted R-ACC-01; its limits and AE-C02–AE-C13 remain outstanding. Earlier SERVER-C2 default/fetch complete library suites PASS (412 each, none failed/ignored), including all 397 original test identities and 15 added regressions/controls. Server all-target Clippy passes in both default and all-feature/fetch profiles. Focused before/after and adjacent results are recorded in the checklist. Historical complete AE-C01 PASS on the unchanged reviewed SERVER-C2 (2,764 Rust tests, none failed/ignored, plus 15 masked-entry terminal tests, documentation and optimized build); AE-C02–AE-C13 remain NOT RUN for general closure; historical core/CLI/server gates do not validate the later accounts delta. No code checks ran during the original preparation phase.
- Documentary checks: file/command-source inspection, Git-diff consistency and the five CLI documentation checks PASS (including local links/anchors, manual/route inventory and split-test declarations). These focused results do not establish a complete software validation control.
- Next action: report the completed local correction/quality commits and Git state, then await instructions. R-ACC-01, its limits, complete gate evidence and per-finding commit mapping are recorded. No push, new audit/reviewer or automatic next pass is authorized.
- Remaining decisions: further independent accounts/server/general-campaign reviews; transport confirmation of SRV-LIFE-2; earlier R-AE009-01 test strengthening/mixed-version acceptance; remaining general-scope audits/controls, release targets, and hardware/client/NAS environments and budgets. Campaign closure/release remain undecided. Evidence stays in the register/checklist.
