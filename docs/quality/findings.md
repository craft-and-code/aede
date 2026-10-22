# Aède Findings Register

## Current campaign

- Campaign: `AE-STAB-001`.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- Preparation date: `2026-10-22`.
- Scope and current permissions: [audit-scope.md](audit-scope.md).
- Severity, confidence, evidence and closure rules: [README.md](README.md).
- First core persistence audit: completed at `ebba04dac6757fe57d3d546367c81e59b5c0f033`.
- Current focused CLI pass: authorized on `2026-10-22`, clean baseline `560fb8675cb7b46635dbdcc44aab8caa44edb38a`.

The maintainer validated AE-001–AE-003, authorized their corrections, then authorized the CLI pass and restoration of this register from the preceding reports. Historical evidence below is attributed to those reports; it is not a claim of fresh reproduction. The former fictitious examples remain excluded.

## Finding index

Allocate stable `AE-*` identifiers sequentially; the next available identifier is `AE-010`. Allocate an ID only to a real recorded defect, hypothesis or material unresolved risk. Reuse it across reviews/campaigns and link duplicates to the existing record.

| ID | Kind | Severity | Confidence | Status | Summary | Detailed record / evidence |
| --- | --- | --- | --- | --- | --- | --- |
| AE-001 | CONFIRMED DEFECT | High | HIGH | VERIFIED | Malformed personal owners silently became local | [Record](#ae-001--malformed-personal-owners-silently-became-local) |
| AE-002 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Analysis content and fallback date could come from different files | [Record](#ae-002--analysis-content-and-fallback-date-could-come-from-different-files) |
| AE-003 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Conclusions-only restore left the server snapshot stale | [Record](#ae-003--conclusions-only-restore-left-the-server-snapshot-stale) |
| AE-004 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Exact-artist fetch selector rejected by the binary | [Record](#ae-004--exact-artist-fetch-selector-rejected-by-the-binary) |
| AE-005 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Worker options bypassed validation on early returns | [Record](#ae-005--worker-options-bypassed-validation-on-early-returns) |
| AE-006 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Human diagnostics and paths interpreted terminal controls | [Record](#ae-006--human-diagnostics-and-paths-interpreted-terminal-controls) |
| AE-007 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Inapplicable options and conflicting operations silently accepted | [Record](#ae-007--inapplicable-options-and-conflicting-operations-silently-accepted) |
| AE-008 | CONFIRMED DEFECT | Medium | HIGH | VERIFIED | Recursive import silently omitted unreadable folders | [Record](#ae-008--recursive-import-silently-omitted-unreadable-folders) |
| AE-009 | UNRESOLVED RISK | Low | MEDIUM | TO CONFIRM | Consent policy for explicitly requested forgetting needs clarification | [Record](#ae-009--consent-policy-for-explicitly-requested-forgetting-needs-clarification) |

## Evidence conventions for recorded passes

Unless a record states otherwise, executed checks ran from the repository root on 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0 (Homebrew), using locked offline dependencies. Results describe the named revision plus its explicitly identified candidate changes. Fixtures use isolated temporary data; no real music library was modified. `PASS` means exit 0; the pre-fix regression failures below exited 101. Counts refer only to the named executed command.

Historical reports preserved in this conversation are the source of the pre-CLI evidence below. The previous aggregate root-gate log was inspected again: it ends with an optimized release build followed by `All green.`. Raw per-regression logs were not committed; the assertions, exact test names and commits retain reproducible evidence. These limitations do not turn historical observations into new runs.

CLI candidate `CLI-C1`: base `560fb8675cb7b46635dbdcc44aab8caa44edb38a` plus the authorized CLI Rust corrections and tests. Source/test fingerprint: `c0fa82e5a4eed42307d9f4b240a8528e557ee0a5fcf8040854ce11fbff468461`. The method sorts all 196 `.rs` paths under `crates/aede-cli/src/` and `crates/aede-cli/tests/` componentwise as Python `pathlib.Path` objects (not lexicographically as full strings), then hashes the concatenation of each repository-relative UTF-8 path, NUL and binary SHA-256 of its contents. Markdown-only recording/manual amendments do not alter that fingerprint. Review records and the checklist identify the eventual commit and final gate.

Intermediate candidate `CLI-C2` uses the same base and fingerprint method: `5a183d31456a01e60f91c13a71525d4a53983c70932175d73c92a7c538d1a700` (196 files). It adds the independently requested known-SGR path regression and caller fixes to CLI-C1. R-CLI-02 found significant AE-008 on this candidate, resetting the clean-review sequence.

Final software candidate `CLI-C3`: same base, 196 Rust files, fingerprint `0d3656c670ce2050467bfab0c5c95334366142ba342249f368cf2c261d580e48`. It adds the recursive-import traversal-error regression and propagation fix. Final reviews and gate must identify CLI-C3; C1/C2 results remain attributed intermediate evidence.

CLI-C3 was committed locally as `4ebf0c7c98ff1498b485f6f19e9e3552cb816ec2` (`fix(cli): enforce option and literal-output contracts`) on 2026-10-22: AE-004–AE-008 production corrections, all 13 regression tests, the intentional main-scope assertion correction and 12 focused English/French manual amendments. Its Rust fingerprint is the reviewed one above. The quality-record commit carrying this entry is documentary bookkeeping; no push or publication is authorized.

Final resolution evidence for AE-004–AE-008: CLI-C3 regression suite PASS (13 default, 11 minimal), final AE-C01 root gate PASS, consecutive post-reset R-CLI-03/R-CLI-04 independent focused reviews reveal no new confirmed significant defect. `VERIFIED` applies to the documented failures/caller fixes, not to every possible CLI combination or the unverified general campaign. AE-009 remains an open policy risk.

Exact fingerprint algorithm, run from the repository root at the recorded candidate:

```python
import hashlib
from pathlib import Path
root = Path("crates/aede-cli")
paths = sorted([*root.joinpath("src").rglob("*.rs"), *root.joinpath("tests").rglob("*.rs")])
digest = hashlib.sha256()
for path in paths:
    digest.update(path.as_posix().encode() + b"\0")
    digest.update(hashlib.sha256(path.read_bytes()).digest())
print(digest.hexdigest(), len(paths))
```

The fresh-context CLI auditor read the parser/scope/dispatch tables and import/export, scan/roots, backup/restore, playlist, merge and spectrum paths; it followed annotations/fetch/sources/missing/integrity selectively, and traced HTTP child arguments and Unix delegation/cancellation across crates. The Test Engineer independently verified the four candidates and added the actual-binary integration suite before production changes. The separate corrector implemented the fixes. These roles were not independent final reviewers. No source author performed either qualifying CLI review.

### AE-001 — Malformed personal owners silently became local

- Identity: AE-001; first campaign AE-STAB-001; `CONFIRMED DEFECT`, `VERIFIED`.
- History (2026-10-22): statically identified on `ebba04d`; independently reproduced before correction; maintainer accepted the finding and correction; verified on the combined candidate later committed as `935774cd295d5d63b7110b45a058b130905c6be1`.
- Location: `crates/aede-core/src/user.rs`, `from_json` owner reads for annotations, relation annotations, plays, counts, collections, set-aside and same-artist; corrected helper `read_owner` at line 1081 in the CLI baseline.
- Contract/component: compatible personal storage and owner-private state, per CLAUDE.md project priorities. Missing legacy owners may default to `local`; present non-string owners must be rejected without publication.
- Severity: High, adopting the independent review's conservative reassessment from the initial Medium: silently assigning private data to another owner compromises ownership integrity. No remote exploitation was demonstrated. Confidence HIGH: deterministic malformed-type cases reproduced acceptance and byte rewriting.
- Trigger: load a version 1 or 2 personal JSON store with a valid target row and an owner of null, boolean, number, object or array; then attempt load-and-save. Preconditions: ordinary local JSON store; no race or special privileges required.
- Expected: reject a malformed owner and preserve original bytes. Actual before correction: all 70 cases (seven tables, five malformed types, two versions) loaded and were saved as `local`. Absent owners and explicit string owners remain accepted deliberately, including strings without a matching current account.
- Tests: `user::tests::malformed_personal_owners_are_rejected_without_rewriting_original_data` and `user::tests::missing_legacy_owners_and_explicit_owners_survive_loading_and_saving` in `crates/aede-core/src/user_tests.rs:48` and `:12`; shared rows in `user_test_support.rs`. Existing personal-storage tests did not protect malformed owner rejection; they were retained.
- Before fix: `cargo test -p aede-core --lib --no-default-features user::tests::malformed_personal_owners_are_rejected_without_rewriting_original_data --locked --offline` — FAIL; the companion missing/explicit-owner control — PASS. Historical report evidence, base `ebba04d` plus tests.
- After fix: `cargo test -p aede-core --lib --no-default-features user:: --locked --offline` — PASS, 86 tests on the combined corrections. Independent review repeated `cargo test -p aede-core --lib user:: --locked --offline` — PASS, 86 tests with default features.
- Correction/disposition: explicit absent/string/invalid matching, propagated before save; owner: campaign implementation agent. Maintainer authorized the fix and local commit. R-CORE-01: CONFIRMED, correction supported. No unresolved High waiver.
- Limits/follow-up: legacy missing owners and orphan string owners intentionally remain supported; invalid target rows remain skippable by the existing reader contract. Reopen on any path that substitutes default personal data after a load error.

### AE-002 — Analysis content and fallback date could come from different files

- Identity: AE-002; first campaign AE-STAB-001; `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED`.
- History (2026-10-22): static path on `ebba04d`; deterministic before-fix reproduction; authorized correction verified and committed as `d43c45b18a318ec7723c28a3e67c4b44324b0623`.
- Location: `crates/aede-core/src/analysis.rs`, `read_report` / `read_report_with_open` at line 589 in the CLI baseline; report import and measurement-date ordering in conclusions.
- Contract: data integrity and deterministic conclusions. A report without an explicit date must obtain content and fallback timestamp from the same opened version.
- Trigger: replace the report pathname after content has been opened/read but before the former separate pathname metadata lookup. Report A has old content, report B has a newer filesystem timestamp. Expected: coherent version or explicit refusal. Actual before fix: A content acquired B's date and could supersede a newer retained measurement. Medium: silently wrong derived conclusions with a local import/reimport workaround; HIGH confidence from the controlled interleaving and code path.
- Existing coverage: explicit dates, conclusions ordering and report persistence did not protect pathname replacement. New tests in `crates/aede-core/src/analysis_tests.rs`: `report_replacement_cannot_promote_an_older_measurement` (line 70, Unix FIFO handshake) and `a_replaced_regular_report_keeps_the_opened_contents_and_date_together` (line 158, regular-file opening seam). Neither uses probabilistic sleeps.
- Before fix: `cargo test -p aede-core --lib --no-default-features analysis::tests::report_replacement_cannot_promote_an_older_measurement --locked --offline` — FAIL, old content was promoted using the replacement date. The regular-file seam did not exist before the correction, so that second regression was NOT RUN before fix.
- After fix: `cargo test -p aede-core --lib --no-default-features analysis::tests:: --locked --offline` — PASS, 36 tests. Independent default-feature `cargo test -p aede-core --lib analysis:: --locked --offline` — PASS, 42 tests; `cargo test -p aede-core --lib conclusions:: --locked --offline` — PASS, 11 tests.
- Correction/disposition: one opened regular-file descriptor supplies bounded bytes and fallback metadata; pre/post length and modification checks reject detectable changes. Maintainer authorized correction and commit; R-CORE-01: CONFIRMED, correction supported.
- Limits/follow-up: the FIFO test now proves early special-file refusal, not ordinary-file coherence by itself; the second deterministic test supplies that evidence. No universal snapshot or hostile-OS guarantee: same-length in-place rewriting with restored timestamps and ancestor filesystem races remain outside this fix. Reopen if a report path is again queried for fallback metadata after reading a different handle.

### AE-003 — Conclusions-only restore left the server snapshot stale

- Identity: AE-003; first campaign AE-STAB-001; `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED`.
- History (2026-10-22): identified on `ebba04d`, reproduced before correction, verified on the combined candidate and committed as `560fb8675cb7b46635dbdcc44aab8caa44edb38a`.
- Location: `crates/aede-server/src/runtime.rs:146` (`StoreStamp`, `storage_stamp`, refresh), `state.rs`, startup/watch paths in `lib.rs`, administrative publication in `admin.rs`.
- Contract: server inspection reflects published core conclusions without restarting. Trigger: running server with cached catalog/conclusions A; publish a supported backup containing only conclusions B, leaving catalog bytes and metadata unchanged; refresh and request the same entity.
- Expected: response contains B. Actual before fix: freshly loaded disk state contained B but the same server router still returned A because reload watched the catalog alone. Medium: stale observable analysis until restart or catalog modification; HIGH confidence from deterministic in-process HTTP observation.
- Existing coverage: catalog replacement/removal and revision reload cases did not protect conclusions-only publication. New regression: `inspection::tests::a_conclusions_only_restore_updates_track_details_without_restarting_the_server`, `crates/aede-server/src/inspection_tests.rs:320`. Uses actual backup serialization and core publication under the store lock, same router/state before and after, unchanged catalog assertions.
- Before fix: `cargo test -p aede-server --lib inspection::tests::a_conclusions_only_restore_updates_track_details_without_restarting_the_server --locked --offline` — FAIL; response retained the pre-restore summary. One earlier mistyped filter ran zero tests and is excluded as verification.
- After fix: the same exact filter — PASS; adjacent `cargo test -p aede-server --lib inspection::tests:: --locked --offline` — PASS (11), `cargo test -p aede-server --lib runtime::tests:: --locked --offline` — PASS (6), and `cargo test -p aede-cli --test backup_safety --locked --offline` — PASS (8).
- Correction/disposition: compare composite catalog/conclusions metadata stamps throughout startup, watcher, reload/failure deduplication and administrative publication. Existing assertions retained; fixture stamp shapes adapted. Maintainer authorized correction and commit; R-CORE-01: CONFIRMED, correction supported.
- Limits/follow-up: HTTP test invokes refresh explicitly; it does not run a whole CLI restore or measure watcher scheduling. Equal metadata stamps can still conceal changes; reload cost from frequent conclusions updates is unmeasured. Reopen for missed store publication paths or observable stale responses despite changed composite stamps.

### AE-004 — Exact-artist fetch selector rejected by the binary

- Identity/assessment: AE-004, AE-STAB-001, `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED`. History on 2026-10-22: confirmed at clean CLI baseline, reproduced independently with tests, corrected in CLI-C1; final CLI-C3 verification under the evidence above.
- Contract/component: parser, validation, help and dispatch must agree (engineering-rules sections 12–13); `docs/cli/fetch.md` and `docs/server/source-review.md` promise exact-artist summaries. Impact: the whole selector route fails, including the child arguments generated by HTTP artist tasks; no provider failure is needed.
- Locations at baseline: `crates/aede-cli/src/args.rs:99` (`VALUED_WORD`), `main.rs:85` (unexpected values), `commands/fetch.rs:1307` (selector), `commands/server_jobs.rs:131` (child argv).
- Trigger: fetch-enabled binary, `aede fetch --summaries --artist-ref=artist:example --dry-run --no-color`. Expected: parse the reference as a single value, then validate/select the exact artist. Observed: exit 2, `option --artist-ref does not accept a value`. Spaced syntax left the reference positional and failed the handler's no-names check, exit 1.
- Before-fix evidence: `cargo build -p aede-cli --locked --offline` PASS; actual temporary-fixture command reproduction above. `cargo test -p aede-cli --test cli_validation --locked --offline` FAIL (exit 101, 0/11 passed overall), including both AE-004 tests. All six store snapshots stayed unchanged for the refusals/previews.
- Existing coverage: exact-artist `fetch_tests.rs` calls `run_with` directly and checks selection/refusals with a fixture transport; generated-job tests inspect argv. Neither protected global main validation.
- Added regression names in `crates/aede-cli/tests/cli_validation.rs`: `exact_artist_summary_selectors_reach_the_handler_in_both_value_syntaxes`; `artist_summary_selector_refusals_name_missing_or_invalid_references`. They assert both real-binary syntaxes reach the intended exact synthetic artist, and missing/non-artist/absent references are refused without rewriting stores; no provider requests.
- Correction/owner: fresh `cli_corrector` agent added `artist-ref` to the existing single-token option family; no new parser or dependency. Targeted combined post-fix command `cargo test -p aede-cli --test cli_validation --test fetch_output --locked --offline` PASS (11 + 5 tests); final evidence/reviews in checklist.
- Limits/follow-up: the synthetic artist has no fetchable provider identifier, so the test stops at deterministic selection/preview. Existing handler tests retain transport/identity coverage; a live provider/native client is NOT RUN. Reopen on any newly generated option absent from the parser. Maintainer authorized correction/local commits; no release decision.

### AE-005 — Worker options bypassed validation on early returns

- Identity/assessment: AE-005, AE-STAB-001, `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED` on CLI-C3. Confirmed, independently reproduced and corrected on 2026-10-22. Auditor proposed Medium; Test Engineer proposed Low. Medium is retained because an invalid resource request can start a full decode successfully, and preview/execution differ; no data loss or actual resource exhaustion was demonstrated.
- Contract: invalid values and unsupported options must fail explicitly (CLAUDE.md section 9). Components/locations at baseline: `commands/check.rs:52` returns into decode before `number_or`; `commands/spectrum.rs:40,66,92` returns empty/preview work before parsing workers.
- Trigger/observation: on a temporary scanned FLAC, `aede check --decode-test --threads=invalid --json` exited 0 with `status:ok`, `verifier:flac -t`; `aede spectrum --dry-run --threads=invalid` exited 0 with one planned image. Decode ignored valid counts too. Expected: refuse unsupported decode workers; validate spectrum values even when it will do no work.
- Existing coverage: `check_tests.rs::invalid_thread_counts_refuse_empty_and_cached_checks_without_changing_stores` covers container checks only. Spectrum generation/failure tests did not cover worker validation on early returns.
- Added regression names in `cli_validation.rs`: `full_decode_refuses_worker_options_it_cannot_honor_without_touching_stores`; `spectrum_refuses_invalid_workers_for_empty_current_and_preview_selections`. Decode tests empty input with valid/invalid explicit counts in JSON and human modes; spectrum tests empty/current/preview paths, unchanged real audio/picture/store bytes and a valid preview control.
- Before fix: shared integration command above FAIL, both tests accepted invalid/unsupported counts. After fix: shared combined command PASS; CLI minimal-feature suite PASS (9 tests; the two fetch-only tests are excluded explicitly).
- Correction/owner: `cli_corrector` refuses any explicit workers with sequential full decode before loading, and parses spectrum workers before early returns. It does not introduce new parallel decoding. Record final gate/reviews in checklist.
- Limits/disposition: no throughput, maximum-worker scheduling or physical decoding acceptance claimed. Existing normal container and generation behavior retained. Maintainer authorized the fix; reopen if a later early return skips validation. No waiver or deferral.

### AE-006 — Human diagnostics and paths interpreted terminal controls

- Identity/assessment: AE-006, AE-STAB-001, `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED` on CLI-C3; confirmed/reproduced/corrected on 2026-10-22, including the independently challenged SGR boundaries.
- Contract: render untrusted text literally before adding styles (engineering-rules section 19; `ui.rs::literal`). Impact: control-bearing arguments or filesystem names can manipulate terminal presentation. No arbitrary code execution or remote exploitation is asserted.
- Locations at baseline: early diagnostics in `main.rs:49,74,153,180,195,242,255`; root mutation success output in `commands/scan.rs:309,318,340`; raw path passed to style-aware `ui::dim` in `commands/reset.rs:53`.
- Trigger/observations: unknown command/option, stray `stats` positional, unsupported-option hints and data hints containing OSC/CSI/BEL/CR/C1 emitted raw control bytes in stderr (exit 2). Excluding/restoring/removing a real temporary control-bearing root emitted them in stdout (exit 0). Expected: visible escaped controls; original persisted paths/audio remain exact. Explicit `--no-color` alone did not sanitize direct interpolation.
- Reset nuance: ordinary CSI/OSC was already escaped by `dim`; recognized SGR was preserved on a pseudo-terminal and dropped under NO_COLOR, contrary to literal-path rendering. The corrector observed the PTY before/after behavior. The Test Engineer separately reproduced the NO_COLOR loss using the retained prior-gate release binary, SHA-256 `bad71149dc089a06d3159658f0ee0004f6657715a96a45ae007d022d8efd4e8e`; provenance is the preceding gate, not a fresh baseline rebuild or embedded commit attestation.
- Existing coverage: annotation/relation/source-prose terminal suites protected imported text, not these early diagnostics and root mutations. Added `cli_validation.rs` tests: `argument_diagnostics_show_terminal_instructions_literally`; `reset_and_missing_data_diagnostics_escape_the_data_path_and_preserve_the_catalog`; `root_changes_escape_displayed_paths_without_rewriting_the_path_or_audio`. The last two are Unix-only because the actual path fixture contains characters invalid on Windows. Reset fixture was strengthened to require literal SGR preservation; raw diagnostic tests also run portably.
- Before fix: shared integration command FAIL for all three tests, with raw escaped-in-log control sequences. Added reset SGR observation is separately attributed above. After fix: combined suite PASS; strengthened exact reset test PASS, exit 0 (1 test, 10 filtered); minimal-feature suite PASS (9).
- Correction/owner: `cli_corrector` applies existing `ui::literal` at identified raw human-output boundaries before presentation. Storage, JSON/CSV and audio are unchanged. No global reformatting or replacement of machine output. Final reviews/gate recorded in checklist.
- Limits/follow-up: no exhaustive audit of every human sentence or every terminal emulator; newline/tab policy remains the documented one. Interactive Windows/native console acceptance NOT RUN. Reopen for another raw untrusted field reaching terminal output. Maintainer authorized correction; no deferral/waiver.
- Independent challenge and further history: R-CLI-01 initially found a Low residual in CLI-C1: recognized SGR still applied or disappeared in scan/roots catalog footers, watched/excluded-root tables, a check scope note and spectrum preview paths. This is deduplicated under AE-006, not a new significant finding. The Test Engineer added `scan_roots_check_and_spectrum_preserve_known_style_tokens_in_literal_paths`; exact command `cargo test -p aede-cli --test cli_validation scan_roots_check_and_spectrum_preserve_known_style_tokens_in_literal_paths --locked --offline -- --exact` FAIL before the additional correction (exit 101, six lost markers, 11 other tests filtered), then PASS after it. It tests six boundaries independently with distinct path markers, cached integrity, unchanged read/preview store snapshots, exact original paths/audio/user bytes and no generated picture.
- CLI-C2 further correction: literalize demonstrated filesystem inputs before `dim`/`Table` in `commands/scan_report.rs`, `scan.rs`, `check.rs` and `spectrum.rs`; no shared presentation API rewrite. Full `cli_validation` PASS (12 default; 10 without default features), CLI all-target Clippy PASS in both profiles. Other raw inputs passed to style-aware presentation helpers in album/credits/import/metadata were noticed statically but not reproduced in this focused review; they remain an explicit broader UI audit limitation, not verified-safe paths or new confirmed findings.

### AE-007 — Inapplicable options and conflicting operations silently accepted

- Identity/assessment: AE-007, AE-STAB-001, `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED` on CLI-C3. Confirmed independently and corrected on 2026-10-22. Related applicability/mode cases share the strict-input failure; numeric early-return validation remains AE-005.
- Contract/impact: answer the requested command or refuse it (CLAUDE.md sections 2/9, engineering-rules section 12). A silently ignored source selector can accompany actual personal-rule creation/removal, not just a harmless listing.
- Locations at baseline: `main.rs::OPTION_SCOPE` lacks compress/quality and `SOURCE_COMMANDS` includes merge; `commands/merge.rs:69` chooses list before forget and never reads source; `commands/discography.rs:328` bypasses window on list and reads remove only under forget around line 545.
- Trigger/actual: `stats --compress=opus --quality=128k`, `merge --list --source=made-up`, `merge --list --forget`, `missing --list --limit=invalid` and `missing --remove` all exited 0. Independent test also observed source-filtered merge creation/forget altering user bytes while ignoring the selector. Expected: explicit refusal before mutation when input cannot be honored.
- Existing coverage: merge identity/owner tests, missing personal decisions/name filters and copy conversion integration cases did not protect these refusal combinations.
- Added `cli_validation.rs` tests: `stats_refuses_copy_encoding_options_instead_of_printing_an_unfiltered_answer`; `merge_refuses_an_unsupported_source_in_every_operation_mode`; `merge_refuses_simultaneous_listing_and_removal_without_changing_the_rule`; `missing_refuses_invalid_list_windows_and_removal_without_forget`. Each refusal protects six store snapshots; valid merge/missing listing controls remain. The existing `main_tests.rs::missing_refuses_source_selection_but_keeps_its_set_aside_operations` intentionally replaces the obsolete merge allowance with an explicit refusal, retaining its other assertions and test identity. This is a changed contract expectation, not removed coverage.
- Before fix: shared 11-test command FAIL; `cargo test -p aede-cli --bin aede tests::missing_refuses_source_selection_but_keeps_its_set_aside_operations --locked --offline -- --exact` FAIL, exit 101 (1 failed, 672 filtered). After fix: combined targeted suite PASS; default/minimal CLI Clippy PASS. Final gate/reviews in checklist.
- Correction/owner: `cli_corrector` restricts encoding flags to copy, refuses merge source selection, conflicting list/forget modes, missing removal without forget and report-only pagination on set-aside modes. Existing domain logic remains in core. Minimal English/French command-reference amendments align the corrected public contract; no unrelated manual rewrite.
- Rejected variant: `sources --source=made-up` already exits 1 because `sources.rs:40–53` requires list/forget/template. Existing missing source refusal also predates this pass. Outdated option-reference caveats are not proof of either code defect; references were corrected, no duplicate finding allocated.
- Limits/disposition: not every command/option pair or repeated-option combination was executed. Last-option-wins behavior is documented in the options reference; no new defect is asserted solely from missing repeated-option tests. Reopen for another demonstrably ignored request. Maintainer authorized targeted refusals and commits; no deferral/waiver.

### AE-008 — Recursive import silently omitted unreadable folders

- Identity/assessment: AE-008, AE-STAB-001, `CONFIRMED DEFECT`, Medium, HIGH confidence, `VERIFIED` on CLI-C3. History on 2026-10-22: independently discovered/reproduced by R-CLI-02 on CLI-C2; Test Engineer reproduced before correction; separate corrector implemented the targeted fix, then AE-C01 and R-CLI-03/R-CLI-04 supported resolution. This new significant defect reset the earlier clean-review sequence.
- Contract/component: CLI import must refuse or report unreadable requested paths (docs/cli/import.md; engineering-rules sections 12/19). Baseline/C2 locations: `crates/aede-cli/src/commands/import.rs:492` (`walk_for_reports`), `:496` (read_dir continue), `collect_reports` and import's merge/save at lines 105–106. Entry iteration via flatten and file_type wildcard also suppressed errors statically.
- Trigger: ordinary Unix user, explicitly requested reports directory containing one readable report and a non-hidden subdirectory containing another valid report, mode 000 with read_dir independently verified as PermissionDenied. Expected: path/context error and nonzero status before publication. Observed by reviewer: exit 0, empty stderr, `Reports read 1` / `Analyses stored 1`, only the accessible report published. Existing data and source reports were not destroyed in that reproduction; silently incomplete ingestion is the Medium impact, with readable retry as workaround.
- Existing coverage: `end_to_end.rs::reports_are_looked_for_in_every_folder_underneath` protects ordinary recursive discovery and empty-directory refusal; `commands/import_tests.rs` protects mode validation. Neither covered traversal failures inside an otherwise readable tree.
- Added regression: `cli_validation.rs::recursive_import_reports_unreadable_folders_before_publishing_and_retries_readable_inputs` (Unix). It checks actual read_dir denial, explicit nonzero/path diagnostic, no success summary and exact preservation of all six store snapshots; after restoring permissions it verifies both new reports, the prior analysis, original personal bytes and catalog roots/files. Permissions are restored before assertions. Privileged Unix users who can read mode 000 exercise a disclosed readable branch, not an unclaimed refusal test; Windows does not run this Unix fixture.
- Before correction command: `cargo test -p aede-cli --test cli_validation recursive_import_reports_unreadable_folders_before_publishing_and_retries_readable_inputs --locked --offline -- --exact --nocapture` FAIL, exit 101 (1 failed, 12 filtered). Actual PermissionDenied branch ran; observed success, missing diagnostic, success summary and changed stores. Readable retry control passed before the aggregate assertion failed. Evidence: local `aede-cli-import-walk-before.log`.
- After correction: same exact test PASS, exit 0, actual PermissionDenied branch; `cargo test -p aede-cli --test cli_validation --locked --offline` PASS (13), same with `--no-default-features` PASS (11); affected existing import unit cases (2) and ordinary recursive E2E case (1) PASS in corrector logs. Default CLI all-target Clippy, format and whitespace PASS. Final gate/reviews are recorded in the checklist.
- Correction/owner: `cli_corrector` changes only production `commands/import.rs` in this step. `walk_for_reports` returns Result, retaining contextual read_dir/entry/file_type errors; collect_reports propagates them before reading/merging/publishing the gathered reports. Final sorting, hidden-name filtering and symlink policy remain intact. The focused import guide amendment documents existing all-input publication semantics explicitly.
- Limits/disposition: runtime reproduction covers read_dir denial; iterator-entry and file_type error propagation are independently inspected static neighbors, not separately injected runtime failures. Trusted-OS/ancestor races remain outside snapshot guarantees. Maintainer authorized confirmed-defect correction and local commits; no waiver/deferral. Reopen on another silently omitted requested subtree or partial publication after traversal refusal.

### AE-009 — Consent policy for explicitly requested forgetting needs clarification

- Identity/assessment: AE-009, AE-STAB-001, `UNRESOLVED RISK`, provisional Low, MEDIUM confidence, `TO CONFIRM`, identified by R-CLI-02 on CLI-C2 and retained on CLI-C3. Confidence concerns whether behavior violates the intended policy; the documentary tension itself is established.
- Locations/contracts: CLAUDE.md section 9 and engineering-rules section 15 require explicit destructive confirmation. `docs/cli/import.md`, `docs/cli/sources.md`, their handlers (`commands/import.rs::forget`, `commands/sources.rs::forget`) and the options reference instead document explicit `--forget` deletion without a separate `--yes` option.
- Scenario/established behavior: an operator deliberately requests import/source forgetting. Import can remove retained measurements that may no longer have source reports; source forgetting can remove review/exclusion decisions as well as fetched records. Neither data class should be described universally as disposable cache. No accidental/implicit deletion, unexpected scope expansion or new confirmed consent bypass was demonstrated.
- Uncertainty/expected behavior: the maintainer must decide whether the explicit named removal operation is itself the intended noninteractive consent, or whether these modes need an additional confirmation. Existing guides/tests support the deliberate operations; the general rule leaves their consent boundary ambiguous. This record is not a demonstrated defect or authorization to change the interface.
- Evidence/tests: static contract/handler examination by the independent reviewer and validator; existing import/source forgetting cases in the normal suites. No standalone policy-bypass reproduction claimed. New regression requirements depend on the policy decision: preserve explicit supported scope, refusal/confirmation semantics, unattended behavior and prior user-data preservation.
- Owner/follow-up/disposition: Aède maintainer; clarify the policy and inventory affected forgetting/removal modes before affected-scope or general campaign closure, then scope any UX/tests/docs change explicitly. Status remains TO CONFIRM, not an accepted deferral or waiver. This question does not block the unrelated verified CLI fixes/local commits and does not reset a significant-defect review sequence; no release or general closure is authorized.

## Allowed finding values

- Kind: `CONFIRMED DEFECT`, `UNVERIFIED HYPOTHESIS` or `UNRESOLVED RISK`.
- Severity: `Critical`, `High`, `Medium` or `Low`; provisional for unverified issues.
- Confidence: `HIGH`, `MEDIUM` or `LOW`, always with a rationale.
- Lifecycle status: one of the statuses below. Status and kind describe different things; a deferred confirmed defect remains a confirmed defect.

| Status | Required meaning / evidence |
| --- | --- |
| TO CONFIRM | A concrete hypothesis or risk exists; state what would confirm or reject it |
| CONFIRMED | Evidence establishes the defect and its triggering execution path |
| IN PROGRESS | Authorized corrective work is underway; the issue is not resolved |
| FIXED, PENDING VERIFICATION | A correction exists at an identified revision, but required verification is incomplete |
| VERIFIED | Required verification supports resolution; include actual results and adjacent-behavior coverage |
| DEFERRED | Record rationale, owner, follow-up and maintainer disposition; the issue remains unresolved |
| REJECTED | Preserve evidence-based justification, including an existing ID if this was a duplicate |

Do not use `VERIFIED` for an unexecuted test plan. A statically confirmed execution path may establish a defect, but it does not establish that a proposed correction passed runtime checks.

## Required detailed record fields

For each allocated ID, add a subsection and complete all fields below. Use explicit unknowns or `NOT RUN` where necessary rather than invented evidence.

### Identity and assessment

- Stable ID, title, first campaign and subsequent campaign references.
- Kind and lifecycle status, with dated status history.
- Reviewed commit, relevant working-tree changes and precise file/line locations.
- Violated contract/invariant and affected component/interface.
- Impact, affected users/data, severity and severity rationale.
- Confidence and rationale; separate established facts from remaining uncertainty.

### Trigger and behavior

- Preconditions: platform, feature profile, permissions, account/session, persisted format and relevant concurrent state.
- Minimal triggering input and ordered steps, including interruption/error timing where relevant.
- Expected behavior and its contract/documentation reference.
- Actual behavior: distinguish runtime observation from behavior inferred from static code paths.
- Reproduction command and outcome if executed; otherwise state `NOT RUN` and the missing prerequisite.

### Evidence and regression requirements

- Supporting code/data-flow references, sanitized inputs, outputs/log excerpts and accessible CI/artifact links.
- For executed checks: exact command, working directory, revision, toolchain/platform, date, exit status and result using the README result vocabulary.
- Missing evidence, environmental limits, alternative explanations and necessary confirmation/rejection steps.
- Existing equivalent coverage and why any additional test is needed.
- Required regression test: observable assertion, fixture/error/concurrency case, why it would fail with the original defect, and adjacent behavior to protect.
- Original-defect reproduction and post-fix verification results, clearly separated; record why an unavailable before-fix run could not be performed.

### Disposition and follow-up

- Correction proposal or reference, owner and affected revision; do not imply authorization from the proposal.
- Resolution verification or deferral/rejection rationale and explicit maintainer decision.
- Residual impact, follow-up action and review trigger/date.
- For an exceptional unresolved High waiver: explicit maintainer acceptance, rationale, compensating measures, accountable owner, follow-up and review trigger. Keep severity/status unresolved and reference the waiver in the final report.
- Related/duplicate IDs and independent review conclusions.

No waiver is available for an unresolved Critical defect. Medium and Low issues require qualification and either verified resolution or justified deferral. See the [closure policy](README.md#campaign-closure-policy).

## Independent review records

Documentation preparation, author self-review and historical milestone reviews do not count toward the required consecutive pair. Review scope is explicit: a clean review of three fixes or of the CLI does not establish coverage of the general campaign.

For each completed review, add a record containing reviewer/context and independence rationale, date, candidate revision/dirty state, distinct focus, actual coverage, evidence examined, challenged findings and verdicts, newly confirmed significant IDs, other new IDs, limitations and required follow-up. Per-finding review conclusions use `CONFIRMED`, `REFUTED`, `PARTIALLY CONFIRMED` or `NOT VERIFIABLE`; these are not campaign verdicts.

| Review reference | Revision | Reviewer/context and focus | New confirmed significant IDs | Coverage / evidence / limitations |
| --- | --- | --- | --- | --- |
| R-CORE-01 | `ebba04d` plus combined AE-001–AE-003 changes, later committed as `935774cd`, `d43c45b1`, `560fb867` | Fresh-context agent `independent_ae_review`, not an author; direct examination of ownership, report coherence and reload paths | None | Three findings CONFIRMED and fixes supported; independent for this focused subset, insufficient for a general-campaign review pair. Detailed evidence below. |
| R-CLI-01 | CLI-C1, refreshed directly on CLI-C2 | Fresh-context `cli_review_1`; parser/dispatch/mode and generated-child-argv focus, plus the full focused CLI scope | None | AE-004/005/007 supported; AE-006's initial Low residual reproduced, tested and corrected; all four CONFIRMED on CLI-C2. First qualifying focused CLI review, not a general-campaign review. |
| R-CLI-02 | CLI-C2 | Fresh-context `cli_review_2`; failures, data preservation, confirmations, human/machine output and delegated lifecycle | AE-008 | Nonclean: recursive traversal error reproduced; resets the clean-review sequence. AE-009 policy risk retained separately. |
| R-CLI-03 | CLI-C3 | `cli_review_1`, first post-reset direct affected-delta review with prior broad scope refreshed | None | AE-004–AE-008 CONFIRMED, fixes supported; actual PermissionDenied regression exercised. First clean focused CLI review after reset. |
| R-CLI-04 | CLI-C3 | `cli_review_2`, separate post-reset review of error publication, preservation and retry, with earlier broad review refreshed | None | Second consecutive clean focused CLI review after reset; independent denied-access reproduction and retry support AE-008. AE-009 remains open. |

### R-CORE-01 — Historical independent correction review

- Date/context: 2026-10-22; fresh agent without inherited conversation, separate from implementation and the preceding author's nonqualifying self-review. No files changed.
- Candidate: combined three-fix working tree based on `ebba04d`; its binary diff SHA-256 was `22be8b74bc97e92d51e687ecc527d8970ab8f7d277d00ed096027482b2958a80`, also checked against the eventual three commits. Commands ran from the root, default features, locked/offline, macOS ARM64 Rust/Cargo 1.99.0.
- Actual tests, all PASS: `cargo test -p aede-core --lib user:: --locked --offline` (86), `cargo test -p aede-core --lib analysis:: --locked --offline` (42), `cargo test -p aede-core --lib conclusions:: --locked --offline` (11), `cargo test -p aede-server --lib a_conclusions_only_restore_updates_track_details_without_restarting_the_server --locked --offline` (1), `cargo test -p aede-server --lib replacing_or_removing_the_catalog_updates_the_snapshot_and_announces_it --locked --offline` (1), `cargo test -p aede-server --lib catalogue_revision_detects_reload_even_with_unchanged_scan_date_and_counts --locked --offline` (1), and `cargo test -p aede-server --lib administrative_scan --locked --offline` (4).
- The administrative-scan command first failed to bind sockets under the sandbox; the same command reran with authorized local socket access and passed all four tests. No production failure was inferred from that initial environmental block.
- Conclusions: AE-001/002/003 CONFIRMED; no new confirmed significant defect. AE-001 severity proposed High rather than initial Medium, adopted here conservatively. Limits: no independent pre-fix execution, no complete CLI restore/watcher timing test, no physical/platform acceptance. Qualifies only for the actual three-fix subset; does not satisfy the two general-scope reviews.

### R-CLI-01 — Independent CLI review and final-delta refresh

- Reviewer/date/independence: `cli_review_1`, 2026-10-22; a fresh context with no implementation/test/audit authorship. Direct source, test and before/after evidence inspection, no file changes. It examined CLI-C1, then re-examined the affected delta and independently recomputed CLI-C2's fingerprint after further corrections; no claim that C1 alone validated C2.
- Actual coverage: entire proposed Rust diff and new tests; parser/aliases/value handling, main validation/dispatch/lock/delegation preflight, generated HTTP argv/staging, check/decode/spectrum, roots/scan/reset, merge/missing, import/sources/rules/annotation mode guards, export protection/CSV, backup/restore confirmation and destinations, and necessary server child/delegation boundaries.
- Initial challenge: AE-004/005/007 CONFIRMED with fixes supported; AE-006 PARTIALLY CONFIRMED because known SGR remained nonliteral in four observed path-output routes. Independent PTY reproduction returned 0 for scan/roots/check/spectrum preview, with raw SGR fixture markers. This was a Low residual of AE-006, not a new significant ID. Other similar UI callers were noted statically only.
- Initial executed checks, all PASS: `cargo test -p aede-cli --test cli_validation --locked --offline` (11); `cargo test -p aede-cli --bin aede args::tests:: --locked --offline` (28); `cargo test -p aede-cli --bin aede tests:: --locked --offline -- --skip commands::` (60, includes those same 28); `cargo test -p aede-cli --bin aede commands::serve::server_jobs::tests:: --locked --offline -- --skip cancellation_still_stops_descendants_after_the_command_leader_exits --skip cancellation_kills_the_whole_child_process_group` (16). The two named process-tree cases were deliberately excluded here, not described as passed by this reviewer.
- CLI-C2 refresh: directly inspected the new regression and four caller files; `target/debug/deps/cli_validation-22e0c25fffb3685c --exact scan_roots_check_and_spectrum_preserve_known_style_tokens_in_literal_paths --nocapture` PASS (1, 11 filtered). This was the current compiled regression binary, invoked directly to avoid the root gate's Cargo lock. Repeated the original real-binary PTY reproduction: scan, roots, check and spectrum preview all returned 0, zero raw fixture markers, with 1/2/1/1 escaped markers respectively. Inspected attributed 12/10-test and lint logs without claiming those as additional own runs.
- Final conclusions: all four IDs CONFIRMED; reported fixes and demonstrated residual supported on CLI-C2. No new confirmed Critical/High/Medium defect. Qualifies as the first independent review of the focused CLI scope, with affected-delta refresh; not the first of a general-campaign closure pair.
- Limits: no independent full gate, complete feature/native matrix, exhaustive UI-call-site audit, every persistence interleaving, real provider, physical output, Windows console or NAS acceptance. Broader unverified styled-input callers remain a scope limitation, not a global safety claim.

### R-CLI-02 — Independent review finding AE-008 and resetting the sequence

- Reviewer/date/independence: `cli_review_2`, fresh context, 2026-10-22; separate from source/test authors and the preceding reviewer; no files changed. Direct examination of all proposed Rust changes/tests and parser/main/fetch/check/spectrum/missing/merge, reset, backup/restore, export/import, Unix delegation, relayed confirmation, cancellation, pipes and HTTP child paths.
- Candidate: CLI-C2 fingerprint independently verified before/after review. Componentwise path ordering was clarified in the fingerprint specification; no source mutation caused the earlier differently ordered digest.
- Conclusions: AE-004–AE-007 CONFIRMED and fixes supported. New confirmed significant AE-008 Medium/HIGH reproduced with actual denied directory access, exit 0 and silent partial publication; this review does not count as a clean second review. AE-009 records an unresolved policy ambiguity, not a confirmed consent bypass.
- Actual executed controls, all PASS: `target/debug/deps/cli_validation-d65c44b4db0bd598 --test-threads=1` (12); `target/debug/deps/aede-e3c41814b25a4116 args::tests:: --test-threads=1` (28), same unit binary with `commands::export::tests:: --test-threads=1` (9), `commands::backup::tests:: --test-threads=1` (7), `commands::serve::server_jobs::tests:: --test-threads=1` (17); `target/debug/deps/end_to_end-0a4407fcda375110 reset --test-threads=1` (3), same E2E binary with `empty_query_and_collection_exports_keep_their_machine_readable_shape --exact` (1), `cancel_without_a_server_refuses_without_changing_cli_data --exact` (1), `delegated --test-threads=1` (2); `target/debug/deps/aede_server-1cb91d2d2499108e delegation::tests:: --test-threads=1` (15); `git diff --check`. Direct compiled binaries avoided a concurrent Cargo build; sockets/process inspection were authorized. These 95 tests passed but did not protect the newly reproduced AE-008.
- Reproduction evidence: independent Python temporary reports with distinct `/waiting/accessible.flac` and `/waiting/unreadable.flac`, actual PermissionDenied check before invoking `target/debug/aede import REPORTS`; restore mode 0700 in finally. CLI binary SHA-256 `d9daab8759dbcc001808dafc0257c5706276df2517b975e4977ceaabd5799a14`; compiled validation binary SHA-256 `9e53a9a61c5d89315c83358ddc7800c32818867be3d7cae17930e406ee5d3877`. Sanitized observed output is retained in AE-008; the Test Engineer separately reproduced it before fixing.
- Limits: no independent pre-fix rerun of older fixes, full feature/native matrix, live provider/device/NAS acceptance or exhaustive terminal-string audit. The full C2 gate was attributed parent evidence, not rerun by this reviewer. Two new clean final-candidate reviews are required after AE-008 correction.

### R-CLI-03 — First independent review after the significant-defect reset

- Reviewer/context/date: `cli_review_1`, 2026-10-22; original context began fresh and has no source/test authorship. Separate from the preceding reviewer; direct import delta/regression/log inspection, no modifications. CLI-C3 fingerprint independently recomputed as recorded above.
- Actual coverage/applicability: earlier broad CLI examination remains applicable because production changes from CLI-C2 are confined to report traversal. Independently verified error propagation through collect_reports before all parsing/merge/save, final sorting and hidden/symlink policy, before/after permission reproduction, six-store preservation and readable retry. Other final CLI changes/tests remain unchanged and supported by the original review.
- Executed `target/debug/deps/cli_validation-22e0c25fffb3685c --nocapture`: PASS, 13 tests, none ignored, explicit `PermissionDenied branch exercised`. Before/after Cargo and corrector lint/unit/E2E logs are attributed inspected evidence, not extra own executions.
- Conclusions: AE-004–AE-008 CONFIRMED with corrections supported. AE-009 NOT VERIFIABLE as a defect, retain provisional Low/MEDIUM unresolved risk for maintainer policy decision. No new confirmed significant defect; this is the first clean focused CLI review after the reset, not a general-campaign review.
- Limits: no exhaustive UI-call-site/filesystem-race audit, independent full build/gate, complete feature/platform matrix, live provider/hardware/NAS acceptance. The separate second review was subsequently completed as R-CLI-04.

### R-CLI-04 — Second consecutive independent review on CLI-C3

- Reviewer/context/date: `cli_review_2`, 2026-10-22; no source/test authorship, originally fresh and separate from reviewer 1. No files changed. Verified CLI-C3 fingerprint before/after; comparison with its C2 manifest showed only `commands/import.rs` and the regression file changed. The twelve existing C2 regressions' prefix remained byte-identical (SHA-256 `21917497166ab1eaca313c3f864d2e98ded5a7f0b390a8c702ad1cc9662a4fa2`). Earlier broad review coverage remains applicable through direct delta/interaction examination.
- Conclusions: AE-004–AE-008 CONFIRMED, fixes supported on C3. Independently checked each new propagation operation before the gather/parse/merge/save boundary, deterministic path sorting, hidden-name skip and symlink-directory policy. AE-009 remains provisional Low/MEDIUM TO CONFIRM, owned by the maintainer. No new confirmed significant defect. This is the second clean focused CLI review after R-CLI-03; R-CLI-02 remains nonclean historical evidence.
- Executed checks: `target/debug/deps/cli_validation-d65c44b4db0bd598 --test-threads=1 --nocapture` PASS (13, none ignored; actual PermissionDenied branch); `target/debug/deps/aede-e3c41814b25a4116 commands::import::tests:: --test-threads=1` PASS (2); `target/debug/deps/end_to_end-0a4407fcda375110 reports_are_looked_for_in_every_folder_underneath --exact` PASS (1); `git diff --check` PASS.
- Independent real-binary Python reproduction: preexisting analysis plus readable report and genuinely unreadable subtree; returned 1, empty stdout, `cannot read report folder <fixture>/reports/unreadable-album: Permission denied (os error 13)`, all six stores unchanged. Readable retry returned 0 and retained all three expected analyses; a hidden directory and directory symlink containing malformed JSON remained ignored. CLI SHA-256 `0af0c34e55e3d4d2ee66cc0eba44947e7073d479fecc3ca7a5a81642f2b58bf6`; validation executable SHA-256 `28d8250b8a1395d0261cfe62f2a1d8284713c02cd03420309159f16c846d4142`. The unchanged E2E harness invoked that rebuilt CLI.
- Limits: directory-entry/file-type error paths inspected statically, not separately fault-injected. No independent duplicate full gate, exhaustive presentation-call-site audit, provider, hardware, Windows console/native matrix or NAS acceptance. This pair covers the focused CLI depth actually inspected, not the entire campaign or release readiness.

A significant defect is a confirmed Critical, High or Medium defect. Record whether each review qualifies under the [independence rules](README.md#independent-reviews). Newly confirmed significant defects reset the consecutive clean-review sequence; retain earlier records. Explain the applicability of both qualifying reviews to the final candidate after any substantive changes.

## Campaign history

No campaign closure has been approved. The completed core persistence pass and current CLI pass are parts of AE-STAB-001. At closure, retain the starting/final revisions, actual scope, new/verified/rejected/deferred IDs, duplicate decisions, executed controls and evidence, two qualifying reviews, remaining risks/acceptance limits and maintainer decision. The final campaign report and separate release decision belong in the [checklist](release-checklist.md#8-campaign-decision-and-final-report).
