# M2 server and security review

Review date: 2026-09-26. Scope: the current working tree, including `aede-server`, CLI dispatch/help/delegation, cancellation, shared store locking and the storage helpers introduced with the server. The initial assessment was read-only; the corrective pass is recorded separately below. The numbered findings describe the original behavior, not the corrected implementation.

## Corrective pass

| Original finding | Correction and regression coverage |
| --- | --- |
| Browser access boundary | Every HTTP request and WebSocket handshake validates a single local Host and, when present, a same-origin Origin. Foreign/null/duplicate authorities and origins are rejected; native clients may omit Origin. |
| Incomplete IPC connection blocks shutdown | Connection handlers are tracked; incomplete frames have an absolute deadline and observe shutdown. Connection/command capacity and slow-output timeouts are bounded. Accepted commands still finish after their client disconnects. |
| Read-side legacy migration races | `store::load` is now read-only. A protected save migrates legacy conclusions before replacing the catalog, preserving results for absent files. Full scans, backups and graph exports preserve embedded conclusions without a source-side write. |
| Saved administrative scan reported as failed | The scan retains its writer lock through loading and publishing the resulting snapshot. A competing writer cannot acquire it in the former gap. |
| Concurrent sidecar replacement | Publication uses an atomic hard link that refuses an existing destination, including a dangling symlink. Tests cover a concurrent creator, competing downloads and complete-file publication. Filesystems without hard links fail closed. |
| Ignored administrative input | Non-empty bodies and query parameters are rejected before starting a task; body reads are bounded and timed. Ambiguous duplicate Authorization headers are refused. |

The CLI help and operating guides now explain startup, data-directory selection, administrative tokens, delegation, interruption and cancellation, permissions, and platform/remote-access limits. Legacy SQLite and integrity-storage statements were corrected. WebSocket connections and incoming messages have explicit caps; application messages are refused and sends have a deadline.

The resolved TLS dependency is updated to `rustls 0.23.45`. `Cargo.lock` is now included in version control, with locked verification and CI/release builds, so another build cannot silently resolve a different dependency set.

Verification after correction: `CARGO_INCREMENTAL=0 tools/check.sh` passes formatting, lint, all Rust tests, documentation checks and the release build on macOS. The disabled incremental cache affects disk usage, not the tests. All 11 offline tests of the advisory-check client pass. Its live OSV lookup passes for 153 locked crates.io package versions, with only the explicitly accepted `paste` maintenance warning; the pinned Git dependency remains visibly outside registry coverage. The new CI workflow is configured, but its hosted execution is not claimed as locally verified.

The sentence above records the state at the time of the initial corrective pass. The later account, HTTPS and playback work is recorded here without changing the historical findings or their evidence.

## Current corrective status

The [account foundation](../design/accounts.md) now implements local account isolation, administrator/user/auditor roles, Argon2id credentials, bounded bearer sessions, expiry/revocation and WebSocket termination. The first administrator retains `local`; personal API operations derive the owner from authentication and recheck it under the writer lock. Version-3 backups include private credentials and rotate the epoch on restore. Account commands stay in the trusted OS-owner CLI and are never delegated.

The default listener remains loopback HTTP. An explicit [HTTPS listener](../server/remote.md) requires initialized accounts, a certificate/private-key pair and one exact public authority. It validates Host and supplied Origin against that authority, accepts only account sessions, and disables every `/api/admin` route and `AEDE_ADMIN_TOKEN`, even for TLS on loopback. The existing catalog and personal API semantics remain additive and unchanged; HTTPS adds a transport boundary rather than a new catalog model. Connection, TLS-handshake, WebSocket and remote-request admission are bounded, so overloaded services can refuse work.

The [native PCM playback contract](../server/playback.md) is a user/admin WebSocket for one current catalogued track. It reuses the shared decode/DSP path, sends processed `f32le`, bounds concurrent decode work to four streams, requires acknowledged client frames and derives private history from that acknowledgement. Auditor sessions cannot start playback. This is deliberately separate from a Subsonic/OpenSubsonic adapter and from any browser/mobile UI.

The [route expansion](../../crates/aede-server/README.md) retains CLI-shaped navigation/inspection and typed administrative scan/fetch jobs. The bodyless scan remains synchronous; an explicitly validated JSON object selects asynchronous work, so the earlier blanket nonempty-body rejection above describes the corrective-pass snapshot, not the extended contract. Unknown fields and unsupported options are still refused. HTTPS deliberately excludes those administrative routes; trusted local HTTP and the local CLI retain their existing administration model.

Navigation/inspection shares bounded blocking work, bounded query complexity and paginated results. Doctor reads current conclusions under the writer lock; corrupt source data is an error rather than a fabricated unknown origin. Personal query predicates and user stores remain outside anonymous reads. HTTP jobs map typed options to existing scan/fetch commands without a shell or arbitrary executable/argument/data-directory input; their capacity, retained records and captured output are bounded. They remain local-installation work, not remote account-owned jobs.

The bounded limits are protections, not a claim that any NAS, browser or real music library has sufficient capacity. Target-NAS connection, memory, CPU and request-cost validation, browser protocol interoperability, client buffering and physical playback remain required before presenting this as a complete remote player deployment.

## M2 hardening follow-up, 2026-10-03

This follow-up covers the current account, HTTPS, native PCM, catalog, personal-data, job and storage paths. The numbered September findings below remain historical evidence.

| Area | Correction and preserved behavior |
| --- | --- |
| Credential and backup reads | Validate the opened descriptor as well as the inspected pathname; reject a changed identity, special file or exposed private credential file. Backup reads are bounded by the opened file's initial size and reject detected size/timestamp/identity changes, without imposing an arbitrary maximum on legitimate large backups. |
| Writer/server locks | Reject links, special files and detected replacement during opening. New Unix lock files are private; existing data-lock permissions are preserved. Server locks also require private permissions and the current owner. |
| Optional stores and graph validation | Only NotFound means an absent optional store. Other metadata failures propagate. Malformed present catalog tables, rows, IDs, foreign keys and inconsistent work/recording links are rejected instead of silently dropped or defaulted. Legacy absent optional tables remain readable. |
| HTTP transport and shutdown | Local HTTP now shares HTTPS parser/admission bounds and write-progress deadlines. Accepted local requests/jobs finish before the final background timeout; upgraded sockets have a bounded drain. Shutdown receivers are installed before first polling so an early signal cannot be lost. |
| PCM acknowledgements | Receive and parse an ACK before applying its progress within the selected branch. Another ready stream branch cannot cancel the receive after consuming an ACK but before updating playback progress/history. Tone validation remains shared with the core. |
| Catalog and personal work | Original heavy catalog routes join the two bounded blocking workers. Search/name/identifier text and references have explicit byte limits. Query and history pagination occurs before rendering/cloning result views; owner counts are indexed once, preserving legacy first-row behavior. |
| Query sorting | Normalize title/album keys and extract numeric values once per result. Stable sorting preserves the supplied selection's order on equal keys; missing numeric values remain last in both directions. |
| Tests and documentation | Reproduce failure cases before fixing them; keep fixtures and tests outside production files. Correct stale CLI playback help, public RustDoc and bilingual operational/API limits. No new dependency or unsafe code is introduced. |

Local HTTP intentionally retains its pre-account compatibility mode: absence of the account store on a fresh process allows anonymous catalog reads. Once that running process observes accounts, later absence fails closed until restart. An unreadable account store always fails closed; HTTPS requires accounts at startup. This is a documented process-local latch, not a persistent activation marker.

The [synthetic HTTP measurements](m2-storage-benchmark.md) compare the previous release and this follow-up on the same generated catalog. They establish a local regression comparison, not NAS capacity, concurrent-client throughput or physical audio behavior. Full verification results are recorded in [current state](current-state.md).

## Outcome and evidence

The initial verification suite passed (`tools/check.sh`: formatting, lint, tests, documentation and release build) despite the original defects. The corrective pass adds regression coverage for these cases; this is why a green suite alone was not sufficient evidence of the boundary. That original conclusion applied before the later account, HTTPS and playback work. The current implementation has an explicit remote transport boundary, but this review does not claim a completed NAS, browser-client or physical-playback validation.

The review combined source inspection, independent HTTP/IPC/help reviews, actual command-help output, bounded local probes on temporary catalogs, and an OSV query for the 153 registry packages in the resolved workspace dependency graph. The probes used an empty temporary music folder or repository fixtures and a fictitious administrative token. Test servers were stopped and their generated data removed. No browser exploit, multi-user OS permission test, resource-exhaustion stress test, or full dependency source audit was performed. Git dependencies were outside the registry advisory query.

## Original findings

### 1. Browser access boundary is not enforced

Priority: high. Confirmed by local protocol probes.

The `events` and `activity` handlers in `crates/aede-server/src/lib.rs` (lines 898–904) accept a WebSocket upgrade without checking `Origin`. A request carrying `Origin: https://attacker.invalid` received `101 Switching Protocols` and a catalog snapshot. The lack of HTTP CORS headers does not enforce the promised WebSocket restriction. A browser page could read task/activity notifications if that browser permits the connection to loopback. Browser policy was not tested and must not be the server's only protection. Validate the handshake origin explicitly; [OWASP's WebSocket guidance](https://cheatsheetseries.owasp.org/cheatsheets/WebSocket_Security_Cheat_Sheet.html#origin-header-validation) describes this requirement.

The router also accepts an arbitrary `Host`: a local GET with `Host: attacker.invalid` returned the library JSON with status 200. This leaves the server without its own defense against DNS rebinding. Acceptance of the hostile authority was reproduced; an end-to-end DNS/browser attack was not. Validate the authority against the intended local address and port, with explicit handling of native clients, before dispatching HTTP or WebSocket requests. Tests should cover foreign hosts/origins, `null` origins, missing origins for native clients, and the intended local authority.

`docs/api.md` currently promises no browser cross-origin access. That promise is stronger than the implementation.

### 2. An incomplete local command connection can prevent shutdown

Priority: medium. Reproduced.

`crates/aede-server/src/delegation.rs` starts an untracked blocking task for each accepted connection (line 197), switches to blocking reads (lines 215–218), and reads a command header without a timeout (lines 406–417). One same-user Unix-socket client that sends no header is enough to keep the runtime alive after SIGTERM. In the probe the server remained alive after two seconds; closing that client let the server exit successfully.

Add a bounded handshake timeout, cancellation of unfinished handshakes during shutdown, and ownership of connection/task lifetimes. Add a regression test that keeps the idle socket open while waiting for server exit. The private socket permissions limit this issue to the same OS user in the documented deployment; this is not evidence of an inter-user permission bypass.

### 3. Legacy migration can write outside the shared writer lock

Priority: medium. Confirmed by source analysis; the race was not reproduced dynamically.

Read commands such as `stats` call `store::load` without the writer lock. When `conclusions.json` is absent, `crates/aede-core/src/store.rs` (lines 219–225) saves migrated conclusions from that read. Two readers can contend for the fixed `conclusions.json.tmp` filename; a delayed reader can also overwrite conclusions that a locked writer has just migrated and enriched. This is limited to the legacy migration window but contradicts the common-writer-lock guarantee.

Make migration an explicitly coordinated operation with a recheck under the lock, or make the ordinary read path pure. Do not simply acquire the same lock inside `store::load`: several callers already hold it. Test simultaneous first reads and a first read overlapping a conclusion-writing command.

### 4. A saved administrative scan can be reported as failed

Priority: medium. Confirmed by source analysis; the timing window was not reproduced dynamically.

The administrative scan releases its write lock before refreshing the server snapshot. If another writer acquires the lock in that gap, `refresh_catalog` returns `Reload::Busy`, leaving the local `known` value unset. `crates/aede-server/src/lib.rs` (lines 1064–1074) then emits `task_failed/catalog_unavailable` and returns 503 even though the scan was saved and the previous snapshot remains available.

Coordinate saving and publishing the resulting snapshot, or distinguish a completed write from a deferred refresh. A regression test must deliberately acquire the writer lock in the gap and verify the reported result, terminal event and eventual snapshot.

### 5. Publishing a new sidecar can replace a concurrently created file

Priority: medium, data integrity. Confirmed by source analysis; no competing-writer stress test was run.

`write_new_atomic` in `crates/aede-core/src/store.rs` checks `path.exists()` (line 175), then uses ordinary `rename` (line 179). Another process can create the destination between those operations, and rename can replace it. Two separate Aède data directories working on the same music tree do not share one lock. A dangling destination symlink also looks absent to `exists()`. The helper's promise to never replace an existing sidecar is therefore not guaranteed.

Use an atomic publication operation that refuses an existing destination, and test both a competing creator and a dangling symlink. Keep the protection against partial final downloads.

### 6. Rescan input is silently ignored

Priority: lower, contract consistency. Reproduced.

The administrative scan handler reads only state and headers. A POST carrying `{"unexpected":"ignored"}` completed the scan and returned 200. The contract says that the endpoint accepts no body. Explicitly decide and test whether non-empty bodies are refused; rejecting them would prevent a caller from believing a requested scope or option had been honored. Cover fixed-length and chunked bodies with a bounded parser.

## Original command-line documentation gaps

The commands exist in the main help index, and command-specific pages show their basic syntax and global options. The behavior is not sufficiently explained from the terminal:

| Help or guide | Missing or misleading behavior |
| --- | --- |
| `aede help serve` | Loopback address and default port; meaning of port 0; initial scan requirement; shared data-directory selection; token setup and its limited scope; lack of accounts/audio/remote access; Unix/Windows limitations. |
| `aede help cancel` | Where task IDs come from, their lifetime, selecting the same data directory/server, which operations are cancellable, and what happens without a server. |
| `aede help scan` / `aede help fetch` | Automatic delegation, continued work after disconnection, and explicit cancellation. |
| Other delegated long commands, including `check` | Ctrl-C detaches the CLI but does not stop the server task; current cancellation supports only scan/fetch. |
| Main help data-directory description | `$XDG_DATA_HOME/aede` is omitted from the actual fallback order. |
| `docs/integrity.md`, interruption paragraph | Describes Ctrl-C as stopping `check` without accounting for delegation; also still describes verdicts as stored in the catalog. |
| `docs/operating.md`, permissions/container advice | Read-only music access is enough for scanning, but delegated artwork/lyrics downloads need sidecar write access for the server account. |
| `docs/design/annotations.md`, SQLite section | Still presents SQLite migration as mandatory in M2, contrary to the current decision. |

The existing help tests mostly verify page presence and dispatch. Add targeted behavioral help assertions for the operational differences above when updating the pages. Existing integration tests cover delegated scan, simulated fetch, client disconnection and cancellation before a scan writes. They do not establish cancellation safety after a fetch has saved a response, shutdown with active delegated work, or the idle-connection case reproduced here.

## Dependency findings

The initial registry advisory lookup reported two matches:

- `rustls 0.23.43`, reached through `ureq 3.4.0`: [RUSTSEC-2026-0285](https://rustsec.org/advisories/RUSTSEC-2026-0285.html), a TLS 1.3 handshake-boundary issue, patched in `0.23.45`, now resolved in the lockfile. This concerns outbound TLS used by fetching, including a delegated fetch; the current HTTP listener itself has no TLS. The advisory does not establish arbitrary handshake forgery by a network attacker.
- `paste 1.0.15`, reached through `lofty 0.25.1`: [RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436.html) reports lack of maintenance, not a demonstrated exploitable flaw. Track the upstream replacement separately.

Registry advisory checks are automated in the read-only [dependency security workflow](../../.github/workflows/security.yml) on pushes, pull requests, weekly and on manual dispatch. Run `python3 -B tools/audit-dependencies.py` for the same explicit network check locally, after `cargo fetch --locked`. Only crate names and versions are sent to OSV. Failed lookups, malformed responses and any non-allowlisted advisory fail the check; the single `paste` maintenance warning is printed rather than hidden. This check remains separate from the offline verification script. An advisory lookup does not assess the project's own code or the contents of Git dependencies.

## Remaining target capacity and client requirements

The later implementation addresses the original account and transport requirements:

- Stable identity comes from the authenticated session; personal routes never accept a client-supplied owner. Administrator, user and auditor permissions are checked on HTTP and WebSocket routes, including playback.
- Session expiry, revocation, logout, password/role changes and WebSocket rechecks are implemented with Argon2id credentials and bounded process-local bearer sessions. Credentials and session tokens stay out of catalog exports and request URLs.
- Remote non-loopback binding is direct HTTPS only, with accounts and an exact authority/origin policy. The local administrative channel remains a same-OS-user capability; HTTPS exposes neither raw commands nor the `/api/admin` family.
- Backup/restore preserves account state and private annotations independently of the JSON catalog format.

The remaining work is deployment and client validation. Admission limits bound connections, TLS handshakes, WebSockets, remote request work and playback decode work, but target-NAS memory, CPU, concurrent-client and large-library behavior still need measured capacity tests. Native clients must still demonstrate browser protocol interoperability where applicable, client buffering/reconnection, certificate handling, session lifecycle and actual device output. Physical playback, latency, seamless joins and target-device performance remain outside this review's evidence. A future Subsonic/OpenSubsonic adapter needs its own compatibility and authorization review.

Retained protections include loopback-only default HTTP, direct HTTPS authority/origin validation, disabled remote administration, private Unix-socket permissions, command validation through the CLI, stripping the admin secret from delegated children, canonical data-directory selection and shared writer locks. Together with the corrective pass these establish an account-backed transport boundary; they do not replace target deployment or client validation.
