# M2 server, persistence and publication safeguards

This combines the server and former M0–M2 interaction reviews. The API contract lives in [API v1](../api.md), the [route inventory](../../crates/aede-server/README.md) and [operating guide](../operating.md); storage design remains in [architecture](../design/architecture.md). [Current state](current-state.md) records qualified verification.

## Authentication and request boundaries

Ordinary HTTP defaults to loopback. Remote non-loopback service uses direct HTTPS, configured authority/origin policy and named accounts. Argon2id credentials, bounded bearer sessions and role checks apply to HTTP/WebSocket operations; expiry, revocation, password/role changes and logout are rechecked. Credentials never enter catalogue exports, request URLs or delegated child arguments. Named administrators may administer over HTTPS; the trusted legacy installation token remains HTTP-local. The public browser root is a static connection/help page, not a private catalogue/account UI.

Browser origin validation, connection/body/frame/time limits, TLS handshake slots and request/decode admission are separate safeguards. Partial Unix command connections cannot indefinitely prevent shutdown. The private Unix socket is a same-OS-user capability with protected permissions; delegated commands reuse typed CLI validation and strip installation secrets from children.

## Shared persistence and private state

All writers and legacy migrations coordinate through the data-directory lock. Reloading under the lock prevents stale mutation; exact canonical entity references are resolved before personal reads/writes without widening a supplied reference to homonymous display names. Alias navigation retains known source attribution.

An administrative scan that saved its catalogue is not reported as failed merely because a later lock/reload step raced. Existing unreadable optional stores and backups remain intact; parts recover independently. Personal-data and account-state backup policy is separate from catalogue format compatibility. See [isolation](account-isolation.md) for final session checks, per-owner capacity and the trusted-operator boundary.

## Analysis and derived-file publication

Stored/imported analysis fields use strict supplied-type/range validation, including bit depth and exact dates. Current-byte identity, source and measured date remain paired through scan attachment, reconciliation, reuse and export. Unknown measurements survive complete native reuse; partial reports cannot suppress complete requested analysis. Legacy unknown dates remain unknown instead of becoming the download/rewrite time. A report's unrelated entry cannot assign another file's date.

Cache invalidation cannot silently omit a selected file or duplicate/override a fresh result when a cache later becomes applicable. Report generation rejects catalogue/private-store output aliases before publication and revalidates the destination. Native task results and CLI batch exits retain partial failures while preserving successfully completed independent albums; an unavailable album folder does not abort useful work in available folders.

Spectrogram caption attempts and fallback render into private staged output with the correct PNG extension. Only successful output is published; final links/special files are refused before work and publication. Failed batches return failure rather than a misleading completed task. Killing a process may leave staging, but partially rendered pictures are not intentionally published as final output.

M3U equality checks admit regular nonlinked destinations, compare the expected length and use an 8 KiB buffer plus EOF validation; FIFOs and oversized files cannot block or allocate their entire contents. Catalogue-derived album/artist M3Us require existing directories and do not recreate unavailable mount paths. Explicit user-chosen export directories retain their intentional creation policy. Ordered selections preserve repeated occurrences; unsafe/ambiguous paths refuse the whole selection. Shared discovery always excludes the private export subtree.

These rules retain the existing filesystem metadata/open and parent-directory TOCTOU limits; they do not establish snapshot isolation or defend against an equally privileged hostile process replacing ancestors.

## Jobs, progressive results and cancellation

Administrative jobs use typed selections, captured roots, bounded process-local state and source/account authorization. Completed provider answers persist as work progresses; a final partial failure does not erase them. Subprocess status and sequential generation retain an earlier nonzero stage outcome. Client disconnection is not task cancellation; server restart is a different registry boundary. See [jobs](../server/jobs.md).

Progressive album decode accepts bounded cumulative completed rows, validated for exact catalogue files, monotonicity, consistency and task ownership. Cancelled/terminal tasks reject later publication, while completed evidence is retained. Intermediate and final diagnostics share credential masking across raw, trimmed, percent-encoded and JSON-escaped spellings, including truncated fragments; generated redaction markers are not rematched as raw source text.

Cancellation tracks owned descendants even after the command leader exits while inherited pipes remain open. On Unix, reserving the leader identity through reader completion permits process-group cancellation without targeting a recycled PID. Independent cleanup attempts retain errors rather than skipping later release steps.

## Dependency and deployment limits

The explicit registry audit is `python3 -B tools/audit-dependencies.py` after locked fetch; it sends package names/versions to OSV. Failed/malformed lookup or non-allowlisted findings fail. The existing `paste` maintenance warning and desktop-specific GTK findings stay visible; [desktop dependency policy](desktop-dependencies.md) describes the exact approved exceptions and corrected Rustls version. Pinned Git sources/external tools are outside registry matching.

Target-NAS memory, CPU, concurrent-client capacity, large-library behavior, service startup/recovery and real client/device buffering still need measured deployment acceptance. Use [storage benchmarks](m2-storage-benchmark.md) rather than assuming SQLite is mandatory or JSON scales without limits. Authentication tests, loopback TLS fixtures and offline checks do not establish acoustic output, every browser protocol or physical renderer interoperability.
