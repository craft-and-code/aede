# AE-STAB-001 — Stabilization and Release Checklist

## Campaign and candidate information

- Project/campaign: Aède / `AE-STAB-001`.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- Preparation date: `2026-10-22`.
- First core review target: `ebba04dac6757fe57d3d546367c81e59b5c0f033`; CLI pass baseline: `560fb8675cb7b46635dbdcc44aab8caa44edb38a`.
- Responsible authority: Aède maintainer; focused-pass and local-commit authorization recorded from the 2026-10-22 instructions.
- Release version, candidate tag and publication targets: not selected.
- Current phase: focused CLI corrections, verification, independent reviews and final evidence validation completed; authorized local commit packaging. No push or release authorized.
- Focused final software candidate: CLI-C3, source/test SHA-256 `0d3656c670ce2050467bfab0c5c95334366142ba342249f368cf2c261d580e48`, based on `560fb867`; method in the register. R-CLI-03/R-CLI-04 form the clean post-reset focused CLI pair. Software remained frozen during those reviews and final validation.
- Software/manual commit: `4ebf0c7c98ff1498b485f6f19e9e3552cb816ec2`; quality records are in the separate documentary commit carrying this checklist. Mapping and final metadata do not change the reviewed software.

The [scope](audit-scope.md) defines commands/applicability and current authorization. The [README](README.md) defines closure policy and evidence vocabulary. This checklist records results and the final report; listing a check does not authorize its execution.

## 1. Source control and change review

Preparation confirmed `main` at the starting commit with a clean working tree before these documentation edits. This observation is not verification of a later code/release candidate.

- [x] The final candidate revision and relevant dirty state are recorded.
- [x] Branch and working-tree state were checked at the initial baseline and writing-role handoffs.
- [x] Unrelated changes were preserved and all final changes are within the authorized scope.
- [x] Intended changes were reviewed and remain traceable in the candidate diff and authorized commits.

## 2. Findings and risk assessment

The [register](findings.md) records AE-001–AE-008 as verified, with actual reproductions, corrections and focused review evidence. AE-009 remains a provisional Low unresolved policy risk. The checks below concern recorded findings, not proof that uninspected code has no defects.

- [x] No recorded unresolved Critical defect remains; no Critical waiver is permitted.
- [x] No recorded unresolved High defect remains; no exceptional waiver is used.
- [x] Recorded Medium and Low defects are qualified and resolved with verification.
- [x] Hypotheses, confirmed defects and material unresolved risks are distinguished; AE-009 remains TO CONFIRM with owner and follow-up.
- [x] Duplicates are consolidated and useful rejection evidence is retained.
- [x] Residual risks and the absence of exceptional unresolved High waivers are visible in the final report.

## 3. Validation results

No software controls ran during documentation preparation. The preceding core pass subsequently ran the root gate successfully on the combined AE-001–AE-003 candidate before its three commits. That result is historical for the CLI pass and must be refreshed after any new correction. Other controls remain unexecuted unless explicitly recorded below; milestone/current-state results are not copied as campaign passes.

Commands are defined once in [scope validation controls](audit-scope.md#validation-controls-and-evidence). Record the exact executed command, working directory, revision/dirty state, toolchain/platform, date, exit status and sanitized output or accessible evidence link for each result. Aggregate gate logs may substantiate their constituent checks without implying additional executions.

| ID | Mandatory software control | Result | Execution evidence |
| --- | --- | --- | --- |
| AE-C01 | Complete root gate: format, lint, tests with FFmpeg, helpers, terminal, versions, site, Rustdoc and optimized build | PASS | Final CLI-C3, 2026-10-22, exact command and retained outcome below; C2 result retained as intermediate evidence |
| AE-C02 | All-feature root workspace tests | NOT RUN | None |
| AE-C03 | Shared audio without default features | NOT RUN | None |
| AE-C04 | Decode-only shared audio | NOT RUN | None |
| AE-C05 | DSP without loudness | NOT RUN | None |
| AE-C06 | Full-feature static analysis | NOT RUN | None |
| AE-C07 | Three minimal-feature lint profiles | NOT RUN | None |
| AE-C08 | Complete independent desktop gate | NOT RUN | None |
| AE-C09 | Native platform/minimum-version software matrix | NOT RUN | None; record each platform/toolchain subresult |
| AE-C10 | Offline advisory-client regressions | NOT RUN | None |
| AE-C11 | Root registry advisory lookup | NOT RUN | None |
| AE-C12 | Desktop registry advisory lookup with existing exact exceptions | NOT RUN | None |
| AE-C13 | Desktop npm advisory lookup | NOT RUN | None |

Use only `PASS`, `FAIL`, `BLOCKED`, `NOT RUN` or `NOT APPLICABLE` as defined in the README. An applicable mandatory software control must pass; missing tools, network or CI access are blockers, not a reason to silently remove it. Any genuine change to applicability requires a documented scope decision. Do not report a partial platform/feature matrix as an aggregate pass.

### Focused CLI execution evidence

All commands below ran at the repository root on 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0, baseline `560fb867` plus the changes explicitly described. These focused controls do not replace AE-C02–AE-C13 or the general campaign scope.

| Stage | Exact command | Result / sanitized evidence |
| --- | --- | --- |
| Clean baseline | `cargo test -p aede-cli --bin aede args::tests:: --locked --offline` | PASS, 28 tests; no production changes |
| Clean baseline | `cargo test -p aede-cli --test server_help --locked --offline` | PASS, 5 tests; dedicated-help exact test also PASS (1) |
| Tests added before fixes | `cargo test -p aede-cli --test cli_validation --locked --offline` | FAIL, exit 101; 0 passed, 11 failed, no ignored; malformed inputs succeeded, selectors failed globally, terminal controls remained raw. Per-finding evidence in register. |
| Scope contract before fixes | `cargo test -p aede-cli --bin aede tests::missing_refuses_source_selection_but_keeps_its_set_aside_operations --locked --offline -- --exact` | FAIL, exit 101; obsolete merge-source allowance still active |
| Corrected candidate before SGR-fixture strengthening | `cargo test -p aede-cli --test cli_validation --test fetch_output --locked --offline` | PASS, 11 + 5 tests |
| CLI-C1 strengthened reset fixture | `cargo test -p aede-cli --test cli_validation reset_and_missing_data_diagnostics_escape_the_data_path_and_preserve_the_catalog --locked --offline -- --exact` | PASS, 1 test, 10 filtered; requires literal preservation of known SGR as well as other controls |
| CLI-C1 minimal features | `cargo test -p aede-cli --test cli_validation --no-default-features --locked --offline` | PASS, 9; two fetch-only tests excluded by cfg, not reported as executed |
| Default CLI lint | `cargo clippy -p aede-cli --all-targets --locked --offline -- -D warnings` | PASS after three mechanical test slice comparisons; assertions preserved |
| CLI-C1 minimal lint | `cargo clippy -p aede-cli --all-targets --no-default-features --locked --offline -- -D warnings` | PASS; conditional imports match fetch tests |
| Format / whitespace | `cargo fmt --all -- --check`; `git diff --check` | PASS; rerun after final changes |
| Independent review's additional SGR case before fix | `cargo test -p aede-cli --test cli_validation scan_roots_check_and_spectrum_preserve_known_style_tokens_in_literal_paths --locked --offline -- --exact` | FAIL, exit 101; all six distinct path-display markers lost, original data preserved |
| CLI-C2 same exact SGR test | Same command as preceding row | PASS, 1 test; other 11 filtered |
| CLI-C2 complete added regression suite | `cargo test -p aede-cli --test cli_validation --locked --offline` | PASS, 12; no ignored |
| CLI-C2 minimal added regression suite | `cargo test -p aede-cli --test cli_validation --no-default-features --locked --offline` | PASS, 10; two fetch-only tests excluded by cfg |
| CLI-C2 lint | Both default and minimal CLI Clippy commands above | PASS; no test assertion changed in the additional correction |

The first sandbox execution of `cargo test -p aede-cli --bin aede --locked --offline` passed 672/673; its descendant-process fixture could not observe the exited leader in that environment. The exact failing test passed with authorized process inspection. Initial `end_to_end` passed 142/144, with two socket startup refusals; both exact cases passed with authorized loopback access. Existing annotation_commands (11), docs (5) and fetch_output (5) passed. No timeout/assertion was weakened. The later complete C3 gate passed in a suitable environment, including 673 CLI unit, 144 E2E and 13 new regression tests; partial initial runs alone were not aggregate passes.

Raw local logs include `aede-cli-before.log`, `aede-cli-before-scope.log`, `aede-cli-corrector-tests.log`, `aede-cli-corrector-clippy.log`, `aede-cli-reset-sgr-before.log`, `aede-cli-reset-sgr-after.log`, `aede-cli-minimal-clippy.log` and `aede-cli-minimal-tests.log` under the local temporary directory. They are ephemeral, not committed artifacts; the commands, outcome excerpts, test names and source identity above retain the technical record without personal paths.

### AE-C01 — Intermediate CLI-C2 root gate

- Exact command: `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh`, repository root, 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0, FFmpeg available and required. Authorized execution included loopback sockets, pseudo-terminals and process inspection required by the existing tests.
- Revision/state: HEAD `560fb867` plus frozen CLI-C2 code/tests and the focused command-manual amendments. No unrelated initial dirty state; final recording-only quality edits do not change the software fingerprint.
- Result: PASS, exit 0. Helper/renderer/version checks, formatting, default-feature all-target Clippy, default workspace tests, real masked-entry terminal tests, generated-statistics/site checks, warning-denied Rustdoc/local links and optimized build all completed. No individual test execution is inferred from the statistics inventory, which explicitly lists tests without running their bodies.
- Sanitized ending: `Finished release profile [optimized] target(s) in 46.94s`, then `All green.` The raw local `aede-cli-final-gate.log` SHA-256 is `b41b2682acfb85b2184b1f0497cf09a0ba8e30438ca0c22de79bafd381e49874`; log retention outside the session is not guaranteed.
- Separately executed documentary check: all eight quality Markdown files' local targets and intra-quality heading anchors PASS; no remote URL or external-document anchor validation claimed. Final diff/whitespace/source-fingerprint checks precede commits.
- This gate does not establish all-feature/minimum-toolchain/native matrix, independent desktop, fresh advisory, live provider or physical acceptance. AE-C02–AE-C13 remain NOT RUN here; earlier scope rules are not waived.

### AE-C01 — Final CLI-C3 root gate and import regression

- Exact command: `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh`, repository root, 2026-10-22, same macOS ARM64 Rust/Cargo 1.99.0 environment, required FFmpeg and authorized local socket/PTY/process access. Base `560fb867` plus frozen CLI-C3 and focused manual amendments. Result PASS, exit 0; all constituent stages completed, optimized build `47.90s`, ending `All green.`
- Final local `aede-cli-final-c3-gate.log` SHA-256: `435db7e4ffb2e09eb1d3845f53d65a74815119e64a81775430f87785214b047a`. It is ephemeral; retained commands, source fingerprint, outcomes and committed regression tests supply reproducible evidence. C2's successful gate did not substitute for this rerun after AE-008.
- New before-fix exact command: `cargo test -p aede-cli --test cli_validation recursive_import_reports_unreadable_folders_before_publishing_and_retries_readable_inputs --locked --offline -- --exact --nocapture` FAIL, exit 101, actual PermissionDenied branch, 1 failed/12 filtered. Same command after correction PASS, actual PermissionDenied branch, no publication on refusal and successful complete retry.
- Final targeted `cargo test -p aede-cli --test cli_validation --locked --offline` PASS (13); same command with `--no-default-features` PASS (11). The two fetch tests are absent from that minimal profile; four real-path/permission tests are Unix-only. Corrector's import unit tests (2), ordinary recursive E2E (1), CLI Clippy, format and whitespace also passed. All 13 default regressions ran again independently in R-CLI-03 and R-CLI-04.
- Directory-entry/file-type failure propagation is statically reviewed without separate fault injection. Current host actually denied mode 000; privileged-host readable branches and unavailable Windows permission coverage are disclosed. This evidence does not complete AE-C02–AE-C13 or certify all CLI inputs/terminals.

### Supplementary and release acceptance

| ID | Separate acceptance activity | Result | Evidence / remaining limitation |
| --- | --- | --- | --- |
| AE-A01 | Windows interactive console/device behavior | NOT RUN | Requires interactive Windows acceptance |
| AE-A02 | Physical gapless, strict ALSA runtime and digital bit-perfect captures | NOT RUN | Requires target hardware and capture setup |
| AE-A03 | Real renderers, live providers and native clients | NOT RUN | Requires selected devices/services/clients and recorded versions |
| AE-A04 | Target-NAS capacity, lifecycle and recovery | NOT RUN | Environment and measurable budgets not yet selected |
| AE-R01 | Candidate platform packaging and release readiness | NOT RUN | Release candidate/targets not yet selected; no publication authorized |

Unavailable supplementary acceptance may remain a documented campaign limit under the agreed scope. It is not a successful check, does not replace mandatory software evidence and may remain a release/deployment blocker.

## 4. Regression verification

- [ ] Significant confirmed fixes have suitable behavioral regression coverage.
- [ ] Tests reproduce the original failure before the fix where practical; unavailable before-fix evidence is explained.
- [ ] Required post-fix and adjacent-behavior checks actually passed at identified revisions.
- [ ] Existing equivalent coverage was considered before adding tests; no test was weakened merely to pass.
- [ ] Unit tests remain in declared sibling `*_tests.rs` files with isolated fixtures, and no test behavior/count was lost through moves or orphaned declarations.
- [ ] Required real encoded fixtures and feature-specific cases remain exercised; skips/ignored cases are disclosed.

## 5. Security, data integrity and Aède contracts

- [ ] Original music/tags remain unchanged and ordinary catalogue operations stay offline.
- [ ] Hostile/truncated input, path aliases, special files and publication failures have been examined within documented filesystem limits.
- [ ] Deterministic graph identity, provenance, conclusions, compatible stores, backup/restore and private state survive relevant errors/migrations.
- [ ] Role/owner isolation, credentials, parental policies, revocation and captured-login checks cover asynchronous preparation and final publication.
- [ ] CLI/API validation, machine output and terminal escaping honor their contracts, including empty/error results and destructive confirmation.
- [ ] Audio feature isolation, bounded queues, complete frames, numerical safety, cancellation, EOF/seek and history/ACK boundaries have been checked.
- [ ] Device discovery, leases, takeover, transport ownership and cleanup have been reviewed without claiming physical interoperability.
- [ ] Desktop private profiles, certificate verification, IPC and child-process lifecycle have been checked independently of the root workspace.
- [ ] Dependency evidence retains approved warnings and explicitly excludes uncovered Git sources/tools; no exception was broadened.

For a genuinely irrelevant subcheck, record `NOT APPLICABLE` and its rationale. Do not check boxes on the basis of an unexecuted plan or undocumented assumption.

## 6. Independent review

- [ ] Two consecutive independent reviews are recorded with reviewer/context, distinct focus, actual coverage, revision and evidence.
- [ ] Both qualify under the [README independence rules](README.md#independent-reviews) and remain applicable to the final candidate after substantive changes.
- [ ] Neither review reveals a new confirmed significant defect: Critical, High or Medium.
- [ ] Newly confirmed Critical, High or Medium hypotheses reset the clean-review sequence; new Low findings are still triaged.
- [ ] Significant fixes, related failure modes and remaining coverage gaps were independently challenged.

Review records: [R-CORE-01](findings.md#r-core-01--historical-independent-correction-review) supports only the three core fixes. CLI-C2's R-CLI-02 found AE-008 and reset the sequence; [R-CLI-03](findings.md#r-cli-03--first-independent-review-after-the-significant-defect-reset) and [R-CLI-04](findings.md#r-cli-04--second-consecutive-independent-review-on-cli-c3) are the consecutive clean CLI-C3 pair from separate originally fresh contexts, with direct affected-delta review and earlier broad-scope applicability explained. The unchecked general-campaign review boxes above remain outstanding; a focused pair does not establish the whole scope. No preparation activity or historical milestone review counts toward it.

## 7. Limits and scope accounting

- [ ] The final report lists actually inspected paths, uninspected areas, exclusions and confidence limits.
- [ ] All applicable mandatory software controls have evidence-backed `PASS` results.
- [ ] Unavailable physical, platform-interactive, provider/client and NAS acceptance is explicitly recorded without claiming success.
- [ ] Whole-process resource, shared-budget, trusted-operator and filesystem TOCTOU boundaries are retained.
- [ ] The maintainer accepts documented residual risks and campaign limitations; any exceptional High waiver is separately identified.

## 8. Campaign decision and final report

Current recommendation: `CONTINUE` — the focused CLI pass has verified corrections and two clean final-candidate reviews, while the general campaign lacks required scope-wide reviews and mandatory software evidence. AE-009 still requires a maintainer policy decision. Campaign closure has not been approved.

At decision time select exactly one of `CONTINUE`, `FIX AND REVALIDATE` or `CLOSE CAMPAIGN` under the [canonical policy](README.md#campaign-verdict-and-release-authorization).

- Final reviewed revision and actual scope: CLI-C3 base/fingerprint above; strict CLI input/dispatch, human/machine output, preservation/import/export, confirmations and delegation at the depth actually recorded. Final commit mapping is retained in the register.
- Commands actually executed and results: historical core evidence plus fresh default/minimal CLI targeted controls and final complete AE-C01 PASS; commands/limits above. All application dependencies remained locked/offline; no fresh advisory lookup claimed.
- New / fixed and verified / rejected / deferred findings: AE-004–AE-008 newly confirmed and verified; AE-001–AE-003 historical resolutions restored and included in the fresh root tests. AE-009 UNRESOLVED RISK, provisional Low/MEDIUM, TO CONFIRM; no accepted deferral or waiver. Rejected source-filter variants are retained under AE-007 without duplicate IDs.
- Qualifying independent review references: R-CLI-03/R-CLI-04 only for the focused CLI scope; R-CORE-01 for the three-core-fix subset. R-CLI-02 remains nonclean; no general-campaign pair established.
- Exceptional unresolved High waivers: none recorded.
- Failed or blocked mandatory controls: none failed on final CLI-C3; AE-C02–AE-C13 NOT RUN prevent general closure. Native/MSRV, independent desktop and fresh advisory evidence remain essential software gaps.
- Residual risks and uninspected areas: AE-009 consent policy; unverified additional raw styled-input callers; exhaustive option combinations, filesystem races, late restore I/O/process interleavings, full playback/device/desktop internals, platform/hardware/provider/client/NAS acceptance. The general campaign extends beyond the inspected core persistence and CLI paths.
- Validator recommendation and supporting rationale: `cli_validator`, fresh read-only context with no source/test authorship, independently checked CLI-C3, gate hash/results, finding histories, review reset/pair and final documentation on 2026-10-22. Recommendation `CONTINUE` for general closure, with no remaining blocker to the two maintainer-authorized local commits. Commit mapping must be recorded without changing the reviewed software. This validator recommendation does not grant release authority.
- Maintainer campaign decision and date: not yet made.

## 9. Separate release authorization

Release authorization: **not granted**. `CLOSE CAMPAIGN` would close only the agreed audit scope and would not authorize tagging, publishing or distributing a release.

Before a release decision, record the selected version/tag/commit, native target artifacts and checksums, release workflow evidence, package contents (including shared crates), applicable client/hardware/deployment acceptance, dependency evidence, unresolved findings and release-specific risk decisions. A campaign High waiver does not itself approve release of that defect. An unresolved Critical prevents release approval.

- Release candidate/version/targets: not selected.
- Applicable acceptance and packaging evidence: not available for this campaign.
- Release blockers/limits: unevaluated candidate; outstanding platform artifacts and separate acceptance activities above.
- Maintainer release decision, rationale and date: not yet made.

Neither this checklist nor a clean review certifies the absence of bugs.
