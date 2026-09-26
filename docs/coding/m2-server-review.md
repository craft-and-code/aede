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

These changes harden the current local service. They do not implement accounts, remote authorization, TLS termination, HTTP query-cost controls or audio playback. The security requirements at the end remain prerequisites for remote exposure.

## CLI-shaped route extension

The subsequent [route expansion](../../crates/aede-server/README.md) adds offline navigation/inspection and typed administrative scan/fetch jobs without changing the local-only access boundary. The bodyless scan remains synchronous; an explicitly validated JSON object now selects asynchronous work, so the earlier blanket nonempty-body rejection above describes the corrective-pass snapshot, not the extended contract. Unknown fields and unsupported options are still refused.

New navigation/inspection shares two blocking-worker slots, bounded query complexity and paginated results. Doctor reads current conclusions under the writer lock; corrupt source data is an error rather than a fabricated unknown origin. Personal query predicates and user stores remain outside unauthenticated reads. Existing original list routes still need broader request-cost controls before remote access.

HTTP jobs require the administrative bearer token for submission, status and cancellation. Typed options map to existing scan/fetch commands without a shell or arbitrary executable/argument/data-directory input; the server account's filesystem and external-service privileges remain substantial. Jobs share the writer lock, survive client disconnection and drain on graceful shutdown. Capacity, retained records and captured output are bounded; service credentials are masked in authenticated results and detailed job failures are not broadcast publicly. There is no durable job queue, account ownership or per-user isolation. These extensions do not change the remote-access prerequisites below.

## Outcome and evidence

The initial verification suite passed (`tools/check.sh`: formatting, lint, tests, documentation and release build) despite the original defects. The corrective pass adds regression coverage for these cases; this is why a green suite alone was not sufficient evidence of the boundary. The local server is not ready for remote exposure or account-based access without further changes.

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

## Security requirements for accounts and remote access

After the local corrective pass, design account access around these requirements:

- Derive a stable user identity from the authenticated session. Never trust an `owner` supplied by a request as authorization. Define administrator and listener permissions and check them on every relevant HTTP operation and WebSocket subscription; test that one user cannot read or cancel another user's private work. See [OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html).
- Define session expiry, revocation, logout, password-change behavior and WebSocket termination together. Use a reviewed password-storage library and secret-generation facility, with explicit dependency choices before implementation. Protect login attempts and keep credentials/session tokens out of ordinary exports and logs. See [OWASP session guidance](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html).
- Make the transition from unauthenticated local reads to account-protected access explicit. Enforce encrypted remote transport and an approved origin/authority configuration before allowing non-loopback binding.
- Retain the new WebSocket/IPC connection, message and timeout limits, and add HTTP connection/query-cost controls before remote exposure. Broad searches/sorts still run synchronously on the runtime; the page-size cap alone does not bound that work. Validate budgets and overload behavior on the target NAS.
- Keep local command execution a same-OS-user administrative channel. It must not become a remotely callable raw-command endpoint. Remote operations need individually authorized APIs, constrained file access, and account-owned task identifiers.
- Preserve and test backup/restore of account state, private annotations and session policy independently of the catalog storage decision. SQLite by itself provides none of the access checks above.

Protections retained include loopback-only binding, disabled-by-default HTTP administration, Origin rejection for administrative POSTs, private Unix-socket permissions, command validation through the CLI, stripping the admin secret from delegated children, canonical data-directory selection and shared writer locks. Together with the corrective pass they are a local-service foundation, not account isolation or a comprehensive security guarantee.
