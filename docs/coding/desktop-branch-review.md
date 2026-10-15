# Desktop integration branch hardening

Review date: 2026-10-15. Scope: the 47 commits from `f7861423` to `d001c496` on `codex/analysis-report-downloads`, and their combined interaction with the existing server, CLI and storage. The target principal branch is `master`. Intermediate commits are preserved; verification targets the integrated result, rather than claiming that all 47 intermediate revisions were independently built.

## Review coverage

| Changesets | Reviewed behavior and boundaries |
| --- | --- |
| `d62d21a8` | Original-entry analysis export, exact track/album scope, stale/partial/version refusal and response bounds. |
| `dbd3dea3` | Filled favourite markers and terminal snapshots. |
| `93782ee1`, `a86fa5f5` | Dynamic browsing grammar, contextual selections and genre unions. |
| `00f05418`, `b01b1252`, `e454e57e` | Disc-aware lightweight track pages, stable references, first-observed dates and one-pass credit search. |
| `8d86f1e6`, `7dcf2d95` | Shared folder M3U planning and grouped private exports without removing history. |
| `8d85ce17`, `60b740d3` | Owned desktop lifetime/bootstrap, unavailable mount retention and deterministic root tests. |
| `d1f97022`, `331b3756` | Per-job service credentials, literal subprocess arguments, bounded output and overlapping-secret redaction. |
| `df309279`, `af09743b`, `4448304f` | Owner-private missing-album decisions, attributed label prose and explicit confined artwork acquisition/cache. |
| `4a65bf46`, `258380a6` | Retained analysis/integrity coverage, complete-set ordering before pagination and bounded sidecar inspection. |
| `49aa7485`, `2c03e024`, `2168c5e1` | Atomic collection renaming, owner-specific rating/tag criteria and exact optional page durations. |
| `b67884e0`, `0f9b0e6e`, `14e25038` | Chronological label playback, explicit enrichment stages, per-answer persistence and partial Discogs failures. |
| `809927cc`, `3841eef6`, `ab74a3f2` | Source-guarded waveform/lyrics hints, owner preferences, advanced query evaluation and repeated ordered selections. |
| `d73f9136`, `269d1632`, `5b28ec98` | Private achievements, per-owner history retention, session rechecks, all-time completion and legacy compatibility. |
| `5889482d`, `066976af` | Press assets/licensing, documentation and generated verification statistics. |
| `babc2002` | DSP programme-meter reuse and fused processing against reference sample/meter behavior. |
| `d8bde1dd`, `586fdba0`, `8d4f6599` | Watched-root M3U discovery/detail/canonical lookup, private-subtree exclusion, descriptor rechecks and inert path resolution. |
| `32bf8861`, `67c44920`, `13f9a268`, `c380929e` | Private mood evidence, shared bounded acoustic descriptors, cooperative foreground priority, cancellation/reuse, source deduplication and separate tests. |
| `73eaecbf`, `4fbfc0c3`, `14081c1a`, `0d46f8e7`, `807b231f` | Exclusive ordered generation jobs, publication locks, exact-file transient decode checks, bounded diagnostics and process-tree lifetime. |
| `c1f2f038` | Confined artist-logo selection using existing image validation. |
| `d001c496` | Exclusive administrator supervision, owner isolation, visibility remapping, quotas/DST, PCM/Subsonic enforcement, backup compatibility and authenticated HTTPS/owned TLS. |

Review included implementation, existing adversarial tests and cross-module callers. No additional defect is asserted merely because a path lacks a new test. Existing bounds, documented filesystem race limits and physical/platform acceptance remain distinct from demonstrated failures.

## Demonstrated defects and corrections

### A mood write could make the common personal store unreadable

With 8192 valid preferences already present, `PUT /api/me/v1/moods` accepted a new preference and returned HTTP 200. The resulting 8193-row `user.json` was refused on its next load, affecting all owners. The generic save boundary also admitted invalid/duplicate preferences which its own reader rejected.

Loading and saving now share preference validation; mood insertion additionally checks the installation and owner capacities. Regression tests prove HTTP refusal, successful rereading at capacity, and byte-for-byte preservation of the previous store on invalid, duplicate or excessive writes. They failed before correction. No existing personal document was opened or rewritten for this audit.

### Closing a PCM connection could refund audio already transmitted

A supervised session could receive one second of PCM, close without an acknowledgement within the same wall-clock second, and leave its usage at zero. Closure now charges the greater of acknowledged and transmitted audio in the current grant, bounded by the grant. These quantities are never added. A received but unplayed prebuffer of up to approximately one second can count toward the quota; listening-history acknowledgement semantics remain unchanged.

Tests reproduce the unacknowledged refund and verify that acknowledging the same audio does not double-charge it. This closes that refund, without claiming to eliminate all abuse of intentionally overlapping voices or the permitted prebuffer.

### Renewing one voice removed another admitted voice's lease

When two leases expired together, renewing the first retired both expired identities. Renewing the second then failed with an unavailable reservation despite sufficient budget. Recent expired identities now remain within the existing eight-slot bound; admission recycles an expired slot only when needed. Their time remains in the usage union, so recycling never refunds it. Tests cover simultaneous renewals, capacity reuse and forgetting identities beyond retention.

### A delayed acknowledgement could charge the previous grant again

After ten transmitted seconds and nine acknowledged seconds, renewal retained an acknowledgement base of nine seconds. A late acknowledgement for the tenth second was therefore treated as new consumption and could produce eleven charged seconds for ten transmitted seconds. The renewed grant uses the already transmitted boundary as its acknowledgement base. The regression drives a fixed clock through the ordinary refresh/load/save path, without a ten-second sleep.

### Continuous retained usage plus prepaid PCM could refuse settlement

A valid continuous 32-day ledger followed by one second of newly prepaid PCM produced an interval longer than the persisted 32-day bound and refused settlement. The retention window now includes the short prepaid horizon, while retaining the existing format/version and interval bound. At most ten seconds of usage already about 32 days old is pruned earlier; current daily and weekly budgets are unaffected. Regression coverage retains the original oversized-interval refusal and checks valid settlement, remaining usage and a validated store roundtrip.

### Cancellation stopped following a completed command's descendants

An administrative command could exit while a descendant retained its stdout/stderr pipes. The runner reaped the leader and then blocked joining readers, no longer observing cancellation. On Unix it now keeps the leader PID reserved until both readers finish, allowing cancellation of the owned process group without targeting a recycled PID. A temporary shell fixture verifies that the leader has exited before requesting cancellation, observes the old timeout, and proves prompt completion after correction. Failure cleanup targets only that fixture's descendant.

## Dependency security

The explicit OSV lookup initially failed for locked `rustls 0.23.45`. A targeted update to `0.23.46` changes only that package version/checksum and its workspace pin; no new product dependency or unrelated package update is introduced.

| Advisory | Upstream condition; no project exploit is inferred from a package match alone |
| --- | --- |
| [RUSTSEC-2026-0346](https://rustsec.org/advisories/RUSTSEC-2026-0346.html) | Debug formatting of client session values in a custom session store can disclose TLS secrets. |
| [RUSTSEC-2026-0347](https://rustsec.org/advisories/RUSTSEC-2026-0347.html) | Enabled ECH, early buffered application data and ECH rejection can send data to the authenticated outer server. |
| [RUSTSEC-2026-0348](https://rustsec.org/advisories/RUSTSEC-2026-0348.html) | Optional Brotli certificate decompression can allocate excessive unauthenticated memory; this build does not enable Brotli. |
| [RUSTSEC-2026-0349](https://rustsec.org/advisories/RUSTSEC-2026-0349.html) | An unauthenticated key-share prediction can downgrade a later hybrid exchange; it is not an authentication bypass. |

The fresh lookup passes for all 244 locked crates.io package versions after correction. The existing `paste` unmaintained warning remains explicitly accepted. The pinned `flaccompagnon-core` Git dependency is outside registry advisory matching; this result does not claim a fresh full upstream codec audit. Eleven offline tests of the advisory client also pass. These results apply to Aède's lockfile, not another application's independent dependencies.

## Verification and limits

Targeted regressions were observed failing before their corresponding fixes. Final verification on macOS Apple Silicon with Rust/Cargo 1.99 passes:

- `tools/check.sh`: 2372 Rust tests, including 2076 active unit tests, 293 integration tests and three doctests, none ignored; required FFmpeg coverage; formatting; warning-free all-target Clippy; helper/site tests; 15 pseudo-terminal account tests; generated statistics and 114 documentation pages in both languages; Rustdoc/site-link checks and the optimized release build.
- `cargo test -p aede-server --no-default-features --locked --offline`: 314 server tests pass.
- `cargo clippy -p aede-server -p aede-core --no-default-features --locked --offline --all-targets -- -D warnings`: passes.
- The explicit registry advisory lookup and its eleven offline helper tests pass with the exclusions described above.

The ten added Rust regressions consist of two preference tests, one cancellation test, four lease/retention tests and three PCM tests. Existing tests were not removed or weakened; the original oversized-interval refusal remains intact. The initial unprivileged test attempt could not bind test sockets; network tests run with loopback access, not with that environment failure hidden or treated as a product defect.

The review preserves original audio/tags, shared catalogue facts and owner-private statements. No new unsafe code, product dependency, user-store format, automatic provider access or source-file mutation is introduced. DSP/reference and cooperative-worker tests establish software behavior, not physical gapless playback, arbitrary OS-load tolerance or whole-app memory acceptance. Filesystem TOCTOU and trusted OS-operator limits remain documented. Native Windows/Linux, NAS deployment, real service credentials and physical audio devices are not newly validated on this Mac. The Rust 1.89 minimum-toolchain build is not rerun here; local execution used Rust 1.99.

This is a branch-integration review. The requested separate M0 and M1 reviews remain subsequent work. Merge preserves every original commit and adds corrective commits; it does not squash or rewrite their history.
