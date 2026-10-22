# AE-STAB-001 — Stabilization and Release Checklist

## Campaign and candidate information

- Project/campaign: Aède / `AE-STAB-001`.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- Preparation date: `2026-10-22`.
- First core review target: `ebba04dac6757fe57d3d546367c81e59b5c0f033`; CLI pass baseline: `560fb8675cb7b46635dbdcc44aab8caa44edb38a`.
- Responsible authority: Aède maintainer; latest 2026-10-22 acceptance authorizes recording R-MOOD-01/limits, complete root verification and local correction/quality commits after successful final diff checks, without push. No campaign closure or release is authorized.
- Release version, candidate tag and publication targets: not selected.
- Current phase: accepted AE-016–AE-019 corrected and behaviorally VERIFIED on frozen MOODS-C1, independently supported by R-MOOD-01. Before/after evidence and three test-depth limits are retained. The maintainer authorized complete root verification and local commits after final diff checks; complete AE-C01 PASS and final diff/staging checks preceded the four local correction/test commits, with results/mapping below. The three quality records are packaged separately. No push or release is authorized.
- Earlier reviewed software candidate: CLI-C3, source/test SHA-256 `0d3656c670ce2050467bfab0c5c95334366142ba342249f368cf2c261d580e48`, based on `560fb867`; method in the register. R-CLI-03/R-CLI-04 form the clean post-reset focused CLI pair. Software remained frozen during those historical reviews and final validation.
- Earlier software/manual commit: `4ebf0c7c98ff1498b485f6f19e9e3552cb816ec2`; earlier quality records committed as `48f15cf44222477a6f4784d974cc924aa98f657a`.
- Earlier candidate: AE009-C1, clean baseline `48f15cf4` plus authorized CLI/server source, regression and documentation changes, uncommitted when reviewed. Expanded 200-file source/test fingerprint `5de300e352a25fc31bc8d62b85d9dea636b7c4c719c3ea3a952547be71d00008`; exact coverage/method in the register. R-AE009-01 is one qualifying independent affected-delta review; its software was frozen during final recording/packaging.
- AE-009 software/manual commit: `e4df4f10ed6d60d1adfcdb146306958f8aa096a7` (`fix(cli): require consent before forgetting data (AE-009)`), 23 reviewed files; the three-file quality record was committed as `5c2fe27f3bdef152c6921ecb8cf8a559f33d0a4d`. Commit mapping/outcome entries do not alter the verified software.

- Earlier server candidate: SERVER-C2, clean server audit baseline `5c2fe27f3bdef152c6921ecb8cf8a559f33d0a4d` plus authorized server source/tests, uncommitted when reviewed. The combined committed software tree is `78e0f01c2f1cf9016a8366e75bd237eb498c68b9`; server quality records were committed as `3919d0422148e5daecd2d44c3728d27fcf04082b`. Three fresh-context correction agents own disjoint files; frozen source/test SHA-256 `ea8b2d4a60a8385db560c7dd5a9e3ad79acbb47ffa1e93e3d9e13ef6157eb4f3` covers all 153 Rust files under `crates/aede-server/src/`; algorithm in the register. Verification is recorded below. R-SRV-01 is one independent affected-subset review; no second server or general-campaign pair is established.

- Earlier candidate: ACCOUNTS-C1, clean baseline `3919d0422148e5daecd2d44c3728d27fcf04082b` plus four authorized Rust files and three quality Markdown files, uncommitted when reviewed. The combined software tree is now `d97d45efb4c41ec5ced85dc64f61ddcd98fa52f9`; AE-014 was committed as `f3d2012915994f3be76679fb4562a3779974f431`. The quality records are packaged separately. Nine-file software fingerprint `47d75ed21bcf6b7dc02935e79bb16d6b406e2b9dd6b8980bd735980527df319d`; coverage/method in the register. The parent reviewed the combined diff and executed validation; the correction/test agents are not post-fix independent reviewers.

- Current candidate: MOODS-C1, clean baseline `04b6a9e9d6fa5bf1a7f5545552610628b437567a` plus the authorized core/moods/server corrections and tests. Source/test SHA-256 `a2152c8a275ad452a9f5b9db3c586e009f4d94c5122aa9b2c3c2ef55fc20d916` covers 173 Rust files; exact coverage/method in the register. It was uncommitted when independently reviewed and is now committed locally as combined software revision `ae7ceb0071627af0a33873a46f95a170798d16c1`. English API documentation and quality Markdown are excluded from that software fingerprint.

The [scope](audit-scope.md) defines commands/applicability and current authorization. The [README](README.md) defines closure policy and evidence vocabulary. This checklist records results and the final report; listing a check does not authorize its execution.

## 1. Source control and change review

Preparation confirmed `main` at the starting commit with a clean working tree before these documentation edits. This observation is not verification of a later code/release candidate.

- [x] The final candidate revision and relevant dirty state are recorded.
- [x] Branch and working-tree state were checked at the initial baseline and writing-role handoffs.
- [x] Unrelated changes were preserved and all final changes are within the authorized scope.
- [x] Final changes are scoped to the accepted corrections, regression fixtures/tests and documentation. Complete gate/final diff checks preceded the four local correction/test/API-document commits; staged paths and bytes matched the reviewed candidate before each commit. The three quality records are packaged separately; previous phase commit prohibitions remain historical.

## 2. Findings and risk assessment

The [register](findings.md) retains AE-001–AE-009 and their historical verification/reviews. The first server pass added maintainer-accepted AE-010–AE-013 (two Medium, two Low, all HIGH confidence), now VERIFIED on SERVER-C2 by the recorded focused/default/fetch checks. The earlier accounts pass added AE-014 and AE-015 (both Low/HIGH), accepted before correction and VERIFIED on ACCOUNTS-C1. The current moods pass adds accepted AE-016 (Medium/HIGH) and AE-017–AE-019 (Low/HIGH); its separate before/after evidence is below. The internal cancelled-scan observation remains unconfirmed at the transport boundary. Earlier test-strengthening/mixed-version/platform limits remain explicit. These checks concern recorded findings, not proof that uninspected code has no defects.

- [x] No recorded unresolved Critical defect remains; no Critical waiver is permitted.
- [x] No recorded unresolved High defect remains; no exceptional waiver is used.
- [x] Recorded Medium and Low defects are qualified and resolved with verification; current AE-016–AE-019 targeted checks pass on MOODS-C1, with earlier results attributed to their candidates. Independent reviews/general closure remain separate.
- [x] Hypotheses, confirmed defects and material unresolved risks are distinguished; AE-009 preserves the original policy ambiguity and the later approved contract, reproduction and verified correction.
- [x] Duplicates are consolidated and useful rejection evidence is retained.
- [x] Residual risks and the absence of exceptional unresolved High waivers are visible in the final report.

## 3. Validation results

No software controls ran during documentation preparation. The preceding core pass subsequently ran the root gate successfully on the combined AE-001–AE-003 candidate before its three commits. That result is historical for the CLI pass and must be refreshed after any new correction. Other controls remain unexecuted unless explicitly recorded below; milestone/current-state results are not copied as campaign passes.

Commands are defined once in [scope validation controls](audit-scope.md#validation-controls-and-evidence). Record the exact executed command, working directory, revision/dirty state, toolchain/platform, date, exit status and sanitized output or accessible evidence link for each result. Aggregate gate logs may substantiate their constituent checks without implying additional executions.

| ID | Mandatory software control | Result | Execution evidence |
| --- | --- | --- | --- |
| AE-C01 | Complete root gate: format, lint, tests with FFmpeg, helpers, terminal, versions, site, Rustdoc and optimized build | PASS | [MOODS-C1 gate](#ae-c01--moods-c1-final-root-gate), 2026-10-22: 2,802 Rust tests, none failed/ignored, 15 terminal tests and all gate stages. Earlier candidate gates remain historical. |
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

### AE-009 — Approved consent policy implementation

- Authorization/revision: on 2026-10-22 the maintainer approved separate consent for import/source forgetting, initially without a commit, then authorized independent review/testing. After R-AE009-01, the maintainer accepted AE-009 and authorized recording its review/limits and local commits after final diff checks. Clean baseline `48f15cf44222477a6f4784d974cc924aa98f657a`, then AE009-C1's changes identified above. No unrelated initial dirty changes; HEAD/index/file contents remained unchanged during review.
- Environment: commands below ran from the repository root on 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0, locked offline dependencies. Actual PTYs, loopback sockets and process inspection ran with the required local permissions. Synthetic temporary fixtures only; no provider requests or real music library changes.

| Stage | Exact command | Result / observed evidence |
| --- | --- | --- |
| Initial regressions before production changes | `cargo test -p aede-cli --test forget_confirmation --locked --offline` | FAIL, exit 101: 1 passed, 6 failed, 0 ignored. Nonconsensual removal, missing prompts, unsupported --yes and no-op store rewrite demonstrated. The later explicit-data-directory control was not yet present. |
| CLI implementation before forwarding flush | `cargo test -p aede-cli --test forget_confirmation delegated_forgetting_obeys_confirmation_and_disconnect_never_consents --locked --offline -- --exact --nocapture` | FAIL, exit 101, 1 failed/7 filtered. Real delegated import displayed counts but buffered the newline-free prompt until the bounded probe timed out. |
| AE009-C1 default regressions | `cargo test -p aede-cli --test forget_confirmation --locked --offline` | PASS, exit 0, 8 passed, 0 ignored; terminal and delegated/disconnected paths exercised |
| AE009-C1 minimal regressions | `cargo test -p aede-cli --test forget_confirmation --no-default-features --locked --offline` | PASS, exit 0, 8 passed, 0 ignored |
| Adjacent CLI verification | `cargo test -p aede-cli --bin aede --test forget_confirmation --test source_operations --test work_source_evidence --test end_to_end --test docs --locked --offline` | PASS, exit 0: 673 unit, 8 consent regressions, 10 source operations, 1 source-evidence, 144 E2E and 5 documentation tests; none ignored |
| CLI/server all-target static analysis | `cargo clippy -p aede-cli -p aede-server --all-targets --locked --offline -- -D warnings` | PASS, exit 0 |
| Complete AE-C01 on AE009-C1 | `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh` | PASS, exit 0: helpers, format, all-target default Clippy, 2,749 Cargo tests (0 failed/ignored), 15 masked-entry terminal tests, statistics/site, Rustdoc and optimized build; ending `All green.` |
| Final pre-commit AE-C01 after review recording | `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh` | PASS, exit 0: all stages, 2,749 Cargo tests (0 failed/ignored), 15 masked-entry terminal tests, optimized build and `All green.`; reviewed software fingerprint unchanged |

- Exact eight regression identities and assertions: [AE-009 record](findings.md#ae-009--consent-policy-for-explicitly-requested-forgetting-needs-clarification). Existing successful deletion fixtures gained explicit `--yes` without assertion removal or test moves. No-op preservation checks use deliberately old store mtimes; real terminal/delegation tests use bounded I/O and a writer fence, without race sleeps.
- Local ephemeral logs: `/tmp/aede-ae009-before.log`, `aede-ae009-delegated.log`, `aede-ae009-targeted.log`, `aede-ae009-minimal.log`, `aede-ae009-cli.log`, `aede-ae009-clippy.log`. Logs are not committed artifacts; exact commands, outcomes and test sources retain reproducibility.
- Implementation AE-C01: PASS, complete run on frozen AE009-C1 with required FFmpeg, same environment and local permissions above. Optimized build completed in `1m 18s`, ending `All green.`; ephemeral log `/tmp/aede-ae009-gate.log` SHA-256 is `2c4242b0cdb6af4541c8ed3da268ab270566758026e0de73cfee4e658191547b`.
- Final pre-commit AE-C01: PASS, exit 0, complete rerun after recording R-AE009-01, 2026-10-22, same repository-root command/environment and required FFmpeg/local permissions. Optimized build completed in `0.17s`, ending `All green.`; ephemeral log `/tmp/aede-ae009-precommit-gate.log` SHA-256 `559e993ea8c4d387b08fc6507faa4faefe682bc96ea169a092f664f754d12d70`. All 23 software/test/manual files and the reviewed 200-file fingerprint remained unchanged. Final eight-file quality link checks passed (90 local targets/Markdown anchors); no remote URL validation. Outcome and commit-mapping entries afterward are documentary bookkeeping only. AE-C02–AE-C13 remain NOT RUN; no independent desktop, platform/MSRV, advisory or hardware/provider/NAS acceptance is inferred.
- Independent review: [R-AE009-01](findings.md#r-ae009-01--independent-consent-policy-review), fresh context `ae009_independent_review`, CONFIRMED Low/HIGH, correction supported, no new confirmed product or significant defect. Parent reran the default/minimal eight-test suites (both PASS, 0 ignored); reviewer directly read those logs and independently ran the two import unit tests, the exact source-forgetting unit test and the real-terminal/server supplemental probe (all PASS). Exact commands, attribution and artifact hashes are retained in the review record. The old executable was not independently rerun.
- Review limits: combined-filter test lacks a same-tool out-of-folder neighbor, delegated test lacks explicit child/parent attestation and previews do not assert every count; the supplemental probe independently passes those behaviors but is ephemeral. Accounts/parental stores are absent in fixtures. Direct commands/server readiness/writer fence lack their own timeout. Mixed client/server versions, Windows/other platforms/MSRV and injected publication failures remain unverified. No new product defect follows from these coverage gaps. No independent full gate/final validator or second AE009-C1 review is claimed.
- Disposition: maintainer accepted AE-009 and authorized local commits without push. R-AE009-01 is a single affected-delta review, not a refresh of both historical CLI reviews or a general-scope pair. General verdict remains `CONTINUE`; no campaign closure or release authorization.

### Server — AE-010–AE-013 accepted corrections

- Authorization/candidate: first read-only audit at clean `main`, `5c2fe27f3bdef152c6921ecb8cf8a559f33d0a4d`, then explicit maintainer acceptance of AE-010–AE-013 and permission for fresh-context fixes, regressions, quality records and targeted execution, without commits. SERVER-C2 is that baseline plus authorized uncommitted server source/tests. Quality-only updates do not change its software fingerprint. Discovery and correction agents are not independent post-fix reviewers.
- Environment/date: repository-root commands on 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0, locked offline dependencies, isolated synthetic accounts/catalogs and loopback sockets. No real music, provider or renderer used. Initial restricted binding attempts were environmental failures; recorded behavioral runs used the required local-socket permission. Intermediate command counts reflect the test sources present at execution while disjoint correction agents worked; final integrated runs identify the frozen candidate separately.

| Stage | Exact command | Result / observed evidence |
| --- | --- | --- |
| Read-only baseline | `cargo test -p aede-server --lib --locked --offline` | PASS, exit 0, 397 passed/0 failed/0 ignored |
| Read-only fetch baseline | `cargo test -p aede-server --lib --features fetch --locked --offline` | PASS, exit 0, 397 passed/0 failed/0 ignored |
| AE-010 durable regressions before fix | `cargo test -p aede-server --lib events::tests:: --locked --offline` | FAIL, exit 101, 5 passed/4 failed/0 ignored; four new matrices failed all 18 refusal cases; healthy traffic control passed |
| AE-010 same command after fix | Same exact command as preceding row | PASS, exit 0, 9 passed/0 failed/0 ignored |
| AE-010 notification neighbors | `cargo test -p aede-server --lib notification --locked --offline` | PASS, exit 0, 14 passed/0 failed/0 ignored; includes credential-queue cancellation/shutdown and notification input/slot handling |
| Intermediate C1 policy-queue boundary | `cargo test -p aede-server --lib events::tests::queued_notification_policy_reads_do_not_block_shutdown_or_peer_disconnection --locked --offline -- --exact` | PASS, exit 0, 1 passed/0 failed/0 ignored; added after fix, no pre-fix claim |
| Intermediate C1 event regressions and strengthened burst | `cargo test -p aede-server --lib events::tests:: --locked --offline` | PASS, exit 0, 10 passed/0 failed/0 ignored; includes queued-read case and 10-socket burst |
| AE-011 crossed HTTP routes before fix | `cargo test -p aede-server --lib bodyless_scan --locked --offline -- --nocapture` | FAIL, exit 101, 0 passed/2 failed/0 ignored; actual 200 and 202 instead of 409; callbacks admitted |
| AE-011 cancelled waiter before fix | `cargo test -p aede-server --lib jobs::tests::aborted_bodyless_scan_waiter_keeps_generation_exclusion_until_worker_stops --locked --offline -- --exact --nocapture` | FAIL, exit 101, 0 passed/1 failed/0 ignored; active callback lost generation exclusion after waiter abort |
| AE-011 four tests after fix | `cargo test -p aede-server --lib bodyless_scan --locked --offline -- --nocapture` | PASS, exit 0, 4 passed/0 failed/0 ignored |
| AE-011 job/admin neighbors | `cargo test -p aede-server --lib --locked --offline -- jobs::tests:: admin::tests:: administrative_scan` | PASS, exit 0, 31 passed/0 failed/0 ignored |
| AE-012/013 before fixes | `cargo test -p aede-server --lib without_mutating_any_owner --locked --offline` | FAIL, exit 101, 0 passed/4 failed/0 ignored; each returned 200 instead of 400 |
| AE-012/013 same command after fixes | Same exact command as preceding row | PASS, exit 0, 4 passed/0 failed/0 ignored, exact store-byte preservation and valid-write controls |
| AE-012 moods neighbors | `cargo test -p aede-server --lib personal::moods::tests:: --locked --offline` | PASS, exit 0, 7 passed/0 failed/0 ignored |
| AE-013 preferences neighbors | `cargo test -p aede-server --lib personal::preferences::tests:: --locked --offline` | PASS, exit 0, 6 passed/0 failed/0 ignored |

Initial integrated SERVER-C1 check: `cargo test -p aede-server --lib --locked --offline` FAIL, exit 101, 409 passed/2 failed/0 ignored on frozen fingerprint `32f4db66e46791a23bb0364579fb60d12ed2805f355d58f04d828a56877515ee` (152 Rust files). Existing `auth::notification_tests::pending_notification_authentication_does_not_block_shutdown_or_peer_disconnection` timed out waiting for credential admission when the added policy stage was already pending; new `events::tests::queued_notification_policy_reads_do_not_block_shutdown_or_peer_disconnection` observed an available permit after retirement because an earlier semaphore reservation did not prove that the future had resumed and queued blocking work. These two failures exposed test synchronization assumptions, not a fifth defect on the clean audit baseline. SERVER-C2 preserves Alice and the original credential-ownership assertions/timeouts while using a sole-async-worker coordinator, continuously free policy admission and blocking-worker gates to prove actual queued work. The newly added policy-queue regression moved into `auth_notification_tests.rs` to share dedicated test-only fixtures; its count and behavior are retained. No original test moved or disappeared. Ephemeral log: `aede-server-c1-default.log`.

C1 correction follow-up: `cargo test -p aede-server --lib auth::notification_tests::queued_notification_policy_reads_cannot_publish_after_logout_or_revocation --locked --offline -- --exact` FAIL, exit 101, 0 passed/1 failed/0 ignored (411 filtered), with C1 production plus the strengthened C2 fixture/test. Both routes published after both direct session termination and protected account revocation while policy validation was queued (all four cases). The coordinator proves event consumption before revocation, so a preceding heartbeat cannot masquerade as the pending event. This is a regression in an uncommitted intermediate correction, retained under AE-010. Logout uses the real session-ending operation directly and revocation uses protected account storage; no HTTP logout request is claimed while the test deliberately occupies the sole blocking worker.

SERVER-C2 policy confirmation/account loading share one admitted worker, with the original final session predicate kept immediately after await. `cargo test -p aede-server --lib auth::notification_tests:: --locked --offline` PASS, exit 0, 4 passed/0 failed/0 ignored on the final source state, including both-route policy-queue retirement and logout/revocation refusal. An intermediate C2 `cargo test -p aede-server --lib notification --locked --offline` also passed 16 tests before the final predicate placement was restored; it is not substituted for final integrated checks. Logs: `aede-ae010-c2-before-auth-policy.log`, `aede-ae010-c2-final-auth-notification.log`, `aede-ae010-c2-notification.log`.

Final integrated SERVER-C2 checks, source/test fingerprint `ea8b2d4a60a8385db560c7dd5a9e3ad79acbb47ffa1e93e3d9e13ef6157eb4f3` (153 Rust files), no production/test edits after freezing:

| Exact command | Result / evidence |
| --- | --- |
| `cargo test -p aede-server --lib --locked --offline` | PASS, exit 0, 412 passed/0 failed/0 ignored/0 filtered; `aede-server-c2-default.log` |
| `cargo test -p aede-server --lib --features fetch --locked --offline` | PASS, exit 0, 412 passed/0 failed/0 ignored/0 filtered; `aede-server-c2-fetch.log` |
| `cargo fmt --all -- --check` | PASS, exit 0 |
| `cargo test -p aede-cli --test docs --locked --offline` | PASS, exit 0, 5 passed/0 failed/0 ignored; local documentation links, manual/route inventory and split-test declarations, `aede-server-c2-docs.log` |
| `git diff --check`; scope/index/HEAD and frozen fingerprint checks | PASS, no out-of-scope edits, no staged changes; HEAD unchanged |
| `cargo clippy -p aede-server --all-targets --locked --offline -- -D warnings` | PASS, exit 0; `aede-server-c2-clippy-default.log` |
| `cargo clippy -p aede-server --all-targets --all-features --locked --offline -- -D warnings` | PASS, exit 0; `aede-server-c2-clippy-fetch.log`; server all-features is its fetch profile |

Final ephemeral library log SHA-256: default `645abea3cf6a492542a30a8259e37056c27edf1c9be11294184e3cf034e07135`; fetch `4b55d44836fe948de1409c7031d950d610c9fb28fe28c78600d2e59534127f88`. These logs and the Clippy/documentation logs above are under the local temporary directory, not committed artifacts. Final result-only documentary recording leaves software and link targets unchanged.

Parent compared executed baseline/default/fetch test identities: all original 397 remain, with 15 new cases and no missing identity. This checks actual run logs; it is not a test-listing count or proof of every assertion. Source/test diff review retains original assertions, with the explicit queued-auth fixture synchronization described above.

Durable regression identities (declared sibling files; no original test removed or assertion weakened):

- `events::tests::established_notifications_close_when_parental_controls_are_enabled`.
- `events::tests::pending_notification_snapshots_close_when_parental_controls_are_enabled`.
- `events::tests::established_notifications_fail_closed_when_an_observed_parental_store_is_unavailable`.
- `events::tests::pending_notification_snapshots_fail_closed_when_an_observed_parental_store_is_unavailable`.
- `events::tests::permitted_notifications_survive_catalog_publications_and_filter_task_events` (adjacent valid control).
- `auth::notification_tests::queued_notification_policy_reads_do_not_block_shutdown_or_peer_disconnection` (moved new async-boundary coverage).
- `auth::notification_tests::queued_notification_policy_reads_cannot_publish_after_logout_or_revocation` (C1 stale-auth regression).
- `jobs::tests::whole_library_generation_excludes_bodyless_scan_until_completion_over_http`.
- `jobs::tests::bodyless_scan_excludes_whole_library_generation_until_completion_over_http`.
- `jobs::tests::aborted_bodyless_scan_waiter_keeps_generation_exclusion_until_worker_stops`.
- `jobs::tests::bodyless_scan_errors_release_generation_exclusion` (adjacent error/cleanup coverage).
- `personal::moods::tests::mood_put_rejects_unknown_query_without_mutating_any_owner`.
- `personal::moods::tests::mood_put_rejects_owner_query_without_mutating_any_owner`.
- `personal::preferences::tests::duplicate_preference_keys_are_refused_without_mutating_any_owner`.
- `personal::preferences::tests::escaped_equivalent_preference_keys_are_refused_without_mutating_any_owner`.

AE-010 refusal matrices cover established/pending streams, both notification routes, catalog lock held/released, policy activation and an observed store missing/corrupt. Healthy delivery now uses ten concurrent sockets across both routes (more than the eight policy readers), gated admission and two catalog publication revisions, preserving event contents and activity-only filtering. A separate deterministic test holds policy permits/the blocking worker and verifies shutdown/disconnect retirement on both routes while queued work retains its permit until exit; no pre-fix execution is claimed for that newly introduced boundary. AE-011 HTTP tests use gated callbacks and valid roots, protect 409/code/no-second-callback and successful readmission; direct waiter-abort coverage establishes exclusion lifetime, not real TCP cancellation. Error cleanup exercises busy writer lock, callback failure, invalid saved catalog and panic. AE-012/013 seed distinct owner state, protect original bytes/other-owner views and accept valid subsequent writes. Arbitrary JSON nested inside a preference value retains its existing duplicate handling; entry keys are the corrected contract.

Ephemeral logs are under the local temporary directory: `aede-ae010-{before,after,notification,policy-queue,final-events}.log`, `aede-ae011-{before-sockets,cancel-before,after,adjacent-after}.log`, and `aede-ae012-013-{before,after,moods,preferences}.log`. The retained test sources and the exact commands/outcomes above retain reproducibility; raw logs are not durable repository artifacts. R-SRV-01 subsequently supports the unchanged candidate with its limits below. No complete root gate or native/MSRV, hardware, real provider/client or NAS acceptance is inferred from these focused checks. Internal cancelled-scan terminal-event behavior remains an unconfirmed transport observation, as retained in the register.

### R-SRV-01 — Accepted independent server review and final packaging

- Review: [R-SRV-01](findings.md#r-srv-01--independent-server-correction-review), fresh-context `server_c2_independent_review`, CONFIRMED for all four findings, corrections supported; no new confirmed defect of any severity. Reviewed unchanged SERVER-C2, fingerprint above, without source/test or preceding-audit authorship. One qualifying affected-subset review; no current pair or general closure.
- Actual regression verification during review: parent default/fetch targeted selections PASS (57 each, all 15 additions included, 0 failed/ignored); documentation PASS (5). Reviewer independently executed adjacent auth/accounts/routing selection PASS (17) and corrected parental/ownership selection PASS (7), 0 failed/ignored. Exact commands, attribution, hashes and the unmatched initial parental filter are retained in the register.
- Accepted limits: snapshot receiver-count rendezvous does not independently isolate the final post-catalog-wait check; generation exclusion during a blocked publication lacks a dedicated admission assertion. Code/guard inspection supports both current paths, so these are strengthening recommendations, not new confirmed defects. SRV-LIFE-2 remains unconfirmed via TCP; native/MSRV, provider/client, physical/NAS and exhaustive failure-injection acceptance remain separate. No source/test changes were made for recording these limits.
- Authorization: on 2026-10-22 the maintainer accepted AE-010–AE-013 and authorized recording R-SRV-01/limits, running `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh` and creating local implementation/test and quality-record commits after final diff verification, without push. The separately executed complete SERVER-C2 root gate PASS is recorded below; previous focused/412-test runs did not substitute for it.

### AE-C01 — SERVER-C2 final root gate

- Exact command: `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh`, repository root, 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0 (Homebrew), FFmpeg installed and explicitly required. Execution had the local socket, pseudo-terminal and process-inspection permissions needed by the existing tests. Cargo remained locked/offline; no dependencies or source/test files changed.
- Revision/state: HEAD/base `5c2fe27f3bdef152c6921ecb8cf8a559f33d0a4d` plus accepted SERVER-C2, before local commits. Frozen 153-file software fingerprint `ea8b2d4a60a8385db560c7dd5a9e3ad79acbb47ffa1e93e3d9e13ef6157eb4f3` unchanged from R-SRV-01. Three quality files recorded the review/limits before this run. Later result/commit-mapping bookkeeping does not change software or verification applicability.
- Result: PASS, exit 0. Helper/renderer/release-coherence checks, formatting, default all-target Clippy, workspace Cargo tests, real masked-entry pseudo-terminals, generated statistics/site checks, warning-denied Rustdoc/local links and optimized build all completed, ending `All green.`.
- Executed Rust evidence: 49 harness summaries, 2,764 passed, 0 failed, 0 ignored, including all 412 server library tests. These counts come from completed test summaries; the later statistics inventory explicitly does not execute test bodies and is not counted. The separate real-terminal suite passed 15 tests. No all-feature/minimum-toolchain/platform matrix is inferred.
- Optimized build ending: `Finished release profile [optimized] target(s) in 1m 21s`, then `All green.`. Ephemeral local log `aede-server-c2-final-gate.log`, SHA-256 `085f8488ce8643d76a8794b066c7f6ba1447c7178f5bcbef84ef84b024d8e12c`; raw retention outside the session is not guaranteed.
- Final checks: exact 15-file scope (12 server Rust implementation/test/fixture files plus three quality files), unchanged reviewed software, no unrelated changes or dependency updates. Commits must retain that fingerprint; result/commit mapping and local documentary checks are bookkeeping only.
- Local commit mapping: AE-010 `fbc8797c35efd66c4a5750f852dd3cfe5fd3126d`; AE-011 `1905988e0e00b80e634bb6412eca98508e77b987`; AE-012 `466bbeaa15700fac9c6de32eaf3643dad2b7698a`; AE-013 `78e0f01c2f1cf9016a8366e75bd237eb498c68b9`. The final combined source tree is the last commit, with all 12 staged source/test/fixture files checked byte-for-byte against the accepted candidate before committing. The remaining three-file quality diff contains only review/evidence/disposition bookkeeping. No push.
- Limits: R-SRV-01 remains one affected-subset review with both test-depth follow-ups; AE-C02–AE-C13 remain NOT RUN. No independent desktop, native/MSRV, fresh advisory, real provider/client/renderer, hardware or NAS acceptance is claimed. Campaign remains CONTINUE; no push or release authorization.

### Accounts correction evidence

All current correction commands ran at the repository root on 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0 Homebrew, baseline `3919d0422148e5daecd2d44c3728d27fcf04082b` plus the identified tests/corrections. The final nine-file ACCOUNTS-C1 fingerprint is in the register. No production code changed between final behavioral executions and documentary recording. Dependencies stayed locked/offline.

The earlier read-only baseline passed 13 domain, four selected core, 11 CLI and 22 server tests (50 total). Its initial restricted server attempt had seven loopback-binding failures; the authorized rerun passed. Diagnostic probes outside the repository caught debug lane-overflow panics in both verifier fields and accepted invalid salts, and demonstrated successful excessive-time mutations followed by core save refusal with unchanged original bytes. The separately executed standard release probe rejected the lane input without a panic. These prior probes exited 0 because they reported/caught the anomaly rather than asserting a regression expectation; they are not passing product regressions.

| Stage | Exact command | Result / evidence |
| --- | --- | --- |
| Added regressions before any logic fix | `cargo test --locked --offline -p aede-accounts --lib` | FAIL, exit 101: 18 passed, seven failed, none ignored. Four AE-014 failures (two lane panics, two accepted-salt cases) and three AE-015 failures (bootstrap/create/administration accepting excessive times). All original 13 tests and five positive new controls passed. |
| Added adapter regressions before any logic fix | `cargo test --locked --offline -p aede-core --lib --no-default-features -- accounts::tests::` | FAIL, exit 101: three original tests passed, four new tests failed, none ignored, 1125 filtered. Lane/corrupt-backup matrices caught panics; salt matrices admitted invalid verifiers; excessive-time revoke returned success. |
| ACCOUNTS-C1 complete domain, debug | `cargo test --locked --offline -p aede-accounts` | PASS, exit 0: 25 unit tests, zero doc tests, none failed/ignored. |
| ACCOUNTS-C1 complete domain, release | `cargo test --locked --offline --release -p aede-accounts` | PASS, exit 0: the same 25 unit tests, zero doc tests, none failed/ignored. Both verifier fields, salt matrix, boundary times and compatibility controls ran. |
| ACCOUNTS-C1 adapter/backup neighbors | `cargo test --locked --offline -p aede-core --lib --no-default-features -- accounts::tests:: backup::tests::credentials_are_independently_versioned_and_old_envelopes_hold_none` | PASS, exit 0: eight tests, none failed/ignored, 1124 filtered. Source/personal bytes, recoverable backup user data, valid credentials and unchanged timestamp refusals verified. |
| ACCOUNTS-C1 CLI neighbors | `cargo test --locked --offline -p aede-cli --test accounts` | PASS, exit 0: 11 tests, none failed/ignored; account/key administration, credential restore and legacy preservation retained. |
| ACCOUNTS-C1 server neighbors | `cargo test --locked --offline -p aede-server --lib -- auth::tests:: accounts_api::tests:: subsonic::authentication::tests:: accounts_api::session_publication_tests:: auth::login_tests::` | PASS, exit 0 with authorized local sockets: 22 tests, none failed/ignored, 390 filtered; owner/role rules, sessions, queued publication and key/policy revalidation retained. |
| ACCOUNTS-C1 targeted static analysis | `cargo clippy --locked --offline -p aede-accounts -p aede-core -p aede-server -p aede-cli --all-targets -- -D warnings` | PASS, exit 0. Package/default feature unification; not the all-feature workspace AE-C06 control. |
| Format | `cargo fmt --all -- --check` | PASS, exit 0. This is not the complete root gate. |
| Documentation and split tests | `cargo test --locked --offline -p aede-cli --test docs` | PASS, exit 0: five tests, none failed/ignored, including documentation links/anchors, route/manual inventory and declared sibling tests. |

The targeted behavioral runs total 91 successful test executions, including the 25-test release repetition. Sixteen tests were added (seven verifier, five timestamp, four adapter); all original 13 domain and three adapter identities/contents were preserved. No existing assertion was weakened, no test moved, and the verifier sibling is declared in `lib.rs`. Malformed-source and backup tests preserve exact source/neighbor bytes and distinguish an unreadable credential part from absence. AE-015 no-op role/status/canonical-name changes intentionally keep their existing success/no-change behavior because no timestamp is published.

Temporary execution logs use the `aede-accounts-c1-*` prefix. Recorded SHA-256 values:

| Temporary log | SHA-256 |
| --- | --- |
| `aede-accounts-c1-cli-after.log` | `d3cc3360bc55215eb33fafe8f84d05caa5f60977f463d6487453a161fcbf0f52` |
| `aede-accounts-c1-clippy.log` | `82a9d7da61b8b045e1f32d6fb960201217c756a665c7430ce21e043763146c68` |
| `aede-accounts-c1-core-after.log` | `2159869a6fe42f7daf5b5cb5b6c827f2980b49185b0aa15c159f813ed3f1435f` |
| `aede-accounts-c1-core-before.log` | `231be4255c7ed2c6355c9aed3783652331caf7dd07cf8cb2089cbd319d6570f8` |
| `aede-accounts-c1-docs.log` | `0495c97fc03fdf45fb7221ef28e5f2016650ee65e1e02c422d4d5fb6913538c8` |
| `aede-accounts-c1-domain-after.log` | `337ebca6a00154a4adec82c23e4a76f1077de4b13344a9e34fcd948d991bf1f8` |
| `aede-accounts-c1-domain-before.log` | `ced5f70392bec69ee4932c4fd88a2c42e38362ca37811884a23ecebb22788759` |
| `aede-accounts-c1-format.log` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `aede-accounts-c1-release-after.log` | `2c406527820226d9d0e7c926f35909d7bb70689731a127708834b2752b6c4d49` |
| `aede-accounts-c1-server-after.log` | `1e42a46f9588365789bb663d4ba2416ac2cd2ee7911b00bc078e740858801ed0` |

They are not committed or a permanent artifact service. Durable test sources plus commands/sanitized failure results preserve reproducibility. R-ACC-01 is recorded below. Complete AE-C01 subsequently PASS on the unchanged software, as recorded below; all-feature server/workspace execution, native Windows/Linux/MSRV, live CLI restore against active sessions/key cache, injected late I/O failures, exhaustive races and real client/provider/NAS acceptance remain NOT RUN for this candidate. No claim of cryptographic proof, real-world timestamp corruption or campaign/release closure is made.

### R-ACC-01 — Accepted independent accounts review and packaging

- The maintainer accepted AE-014/AE-015 and authorized recording [R-ACC-01](findings.md#r-acc-01--independent-accounts-correction-review), its limits, complete root verification and local commits after final diff checks, without push. The reviewed ACCOUNTS-C1 software remains frozen at the nine-file fingerprint above; documentary evidence/commit mapping does not change it.
- Fresh-context `accounts_c1_independent_review` is separate from discovery/source/test authors and earlier reviewers. AE-014 and AE-015 are CONFIRMED, Low/HIGH, corrections supported; no new confirmed product defect or new ID. One qualifying accounts-subset review is recorded, not a second accounts/general-scope review or campaign closure.
- Reviewer execution: `cargo build --locked --offline -p aede-accounts --message-format=json` PASS; `cargo test --locked --offline -p aede-core --lib --no-default-features -- backup::tests::` PASS (17 tests, none failed/ignored, 1115 filtered); stdin-compiled diagnostic PASS (152 verifier cases, real password/key authentication for five compatible parameter layouts, 20 atomic refusal cases plus no-op controls). Exact method/command/binary hash are in the register.
- Parent execution during review: the five targeted behavioral commands above PASS again (25 debug/25 release/8 core/11 CLI/22 server), plus five documentation tests and formatting/diff checks. These were current tool outputs, not fresh log files; the reviewer read older implementation logs separately as historical evidence. No command is attributed to an actor who did not run it.
- Limits retained: durable auxiliary-layout tests parse dummy hashes, while real authentication is verified only by the supplemental probe; malformed-backup regressions verify parts/source preservation rather than live CLI restore/session/key-cache behavior. These are coverage follow-ups, not current product defects. No independent complete gate, native/MSRV matrix, exhaustive malformed-input/RNG/late-I/O/race validation or hardware/provider/client/NAS acceptance is implied. Complete root execution below is parent validation, not a reviewer execution.

### AE-C01 — ACCOUNTS-C1 final root gate

- Authorization/candidate: after accepted R-ACC-01, the maintainer requested complete verification before local commits. Root command `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh`, repository root, 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0 Homebrew and installed FFmpeg, with authorized local sockets, pseudo-terminals and process inspection. Dependencies remained locked/offline.
- Result: PASS, exit 0, ending `All green.`. Cargo unit/integration/documentation runs passed **2,780 tests**, zero failures/ignored, across 49 reported suites. The separate real-terminal helper passed **15 tests**. This total excludes the later test-inventory enumeration, which explicitly does not execute test bodies.
- Constituents: build-helper and website/release checks, formatting, default all-target Clippy, required FFmpeg conversions, complete root Cargo tests, masked account entry, generated source/test inventory and 116 bilingual documentation pages, local links/fragments/SEO, Rustdoc with warnings denied and the optimized release build all passed. The final optimized build completed in `1m 26s`.
- Frozen software: nine-file ACCOUNTS-C1 SHA-256 `47d75ed21bcf6b7dc02935e79bb16d6b406e2b9dd6b8980bd735980527df319d` remains unchanged from R-ACC-01; all 16 additions and original coverage remain. No source, tests, dependencies or configuration changed while recording review/gate evidence or packaging commits.
- Local commit mapping: AE-014 `f3d2012915994f3be76679fb4562a3779974f431` (verifier correction and ten regression/control tests), AE-015 `d97d45efb4c41ec5ced85dc64f61ddcd98fa52f9` (timestamp correction and six regression/control tests). The combined source tree matches all nine reviewed/gate-verified files. Only three Markdown quality records are packaged separately; no other source or generated files are included. No push.
- Evidence: ephemeral log `/tmp/aede-accounts-c1-final-gate.log`, SHA-256 `b9899024ba788fd4172e3f5a3f823ca12cd8c21c7b10180a9514a507a69a9cad`. Raw logs are not committed; command/results, durable tests and candidate fingerprint retain reproducible evidence. Outcome/commit mapping after this run is Markdown bookkeeping; local quality links and the final diff are rechecked before the quality commit.
- Limits: this is parent validation, not another independent review or an all-feature/platform/advisory/desktop control. AE-C02–AE-C13 remain NOT RUN, as do supplementary hardware/provider/client/NAS acceptance. R-ACC-01's two test-depth follow-ups remain explicit. Campaign stays CONTINUE; no push or release is authorized.

### Moods correction evidence

Historical correction-stage permissions: accepted AE-016–AE-019, corrections/regressions/records/targeted checks only, initially without commits. Later acceptance after R-MOOD-01 separately authorizes complete root validation and local commits after final diff checks, without push. All commands ran at the repository root on 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0 Homebrew, locked/offline dependencies, baseline `04b6a9e9d6fa5bf1a7f5545552610628b437567a` plus the identified test/correction stage. Local sockets were explicitly authorized for HTTP/server tests. No real library, external service or process-global test state was used. All Cargo executions below are parent executions; correctors ran formatting/diff inspection only.

| Stage | Exact command | Result / observed evidence |
| --- | --- | --- |
| AE-019 tests before semantic fix | `cargo test --locked --offline -p aede-core --lib --no-default-features -- audio_features::budget_tests::` | FAIL, exit 101; two EOF cases failed, two PCM/boundary controls passed, none ignored, 1132 filtered. The private source/clock refactor retained the faulty EOF branch for this run. |
| AE-016 tests before fix | `cargo test --locked --offline -p aede-server --lib -- audio_features::ae016_tests::` | FAIL, exit 101; six changed/missing-source cases failed, unchanged-source control passed, none ignored, 416 filtered. Three hot views stayed ready; three cold warms retained stale records. |
| AE-017 domain before fix | `cargo test --locked --offline -p aede-moods --lib measured_recipe_ids` | FAIL, exit 101; one test failed/13 filtered, none ignored; a measured ID passed semantic-rule validation. |
| AE-017 HTTP before fix | `cargo test --locked --offline -p aede-server --lib -- measured_recipe_ids preference_patches_cannot_replace_measured mood_preference_json persisted_reserved` | FAIL, exit 101; three failed, untyped-preference control passed, none ignored, 423 filtered; all four PUT and eight PATCH attempts returned 200 instead of 400; stored IDs suppressed measured recipe fields. |
| AE-017 normalized legacy rerun before fix | `cargo test --locked --offline -p aede-server --lib persisted_reserved_moods_are_reported_without_replacing_recipes_or_losing_preferences` | FAIL, exit 101; one failed/427 filtered, none ignored; all four legacy collisions selected a descriptor-less source (`total=1`, `acoustic=null`). The initial fixture used a hyphenated tag without applying normalization; it already exposed missing measured fields but not the intended tag/vocabulary effect. The fixture was corrected before production changes; expected assertions were preserved. |
| AE-018 tests before worker fix | `cargo test --locked --offline -p aede-server --lib audio_features::worker::tests -- --test-threads=1 --nocapture` | FAIL, exit 101; four failed/423 filtered, none ignored; actual resident keys retained old catalogues, including unavailable replacement and cancellation/reuse; queued reload retained obsolete paths. AE-016 was already being corrected, while the worker still had its original publication/pruning behavior. |
| AE-019 core extraction/cache after fix | `cargo test --locked --offline -p aede-core --lib --no-default-features -- audio_features::` | PASS, exit 0; nine passed/1127 filtered, none failed/ignored, including all five original tests and four new budget cases. |
| Combined server affected paths | `cargo test --locked --offline -p aede-server --lib -- audio_features:: personal::moods::tests:: personal::preferences::tests::` | PASS, exit 0; 40 passed/389 filtered, none failed/ignored. Includes all 17 new server tests and preserved owner/supervised/admission/paging/preferences coverage. A subsequent integration guard keeps the catalogue read lock through warm publication; the final server check below covers that adjustment. |
| Moods complete crate after fix | `cargo test --locked --offline -p aede-moods` | PASS, exit 0; 14 unit tests passed, none failed/ignored; zero doc-tests. All 13 original tests retained, one rule-ID regression added. |
| Final complete default server library | `cargo test --locked --offline -p aede-server --lib` | PASS, exit 0; 429 passed, none failed/ignored, zero filtered, in 57.04 seconds. Covers the integration publication guard and all 412 original server tests plus 17 additions. |
| Targeted all-target static analysis | `cargo clippy --locked --offline -p aede-moods -p aede-core -p aede-server --all-targets -- -D warnings` | PASS, exit 0. Default package features, including core fetch; not the workspace all-feature/minimal matrix. |
| Documentation contracts | `cargo test --locked --offline -p aede-cli --test docs` | PASS, exit 0; five tests, none failed/ignored: local links/fragments, manual/route inventory and split-test declarations. Rechecked after final Markdown recording. |
| Targeted public API documentation | `RUSTDOCFLAGS='-D warnings' cargo doc --locked --offline -p aede-moods -p aede-core -p aede-server --no-deps` | PASS, exit 0. |
| Final formatting | `cargo fmt --all -- --check` | PASS, exit 0, after mechanical formatting of changed AE-016 files; the initial check reported formatting differences. |
| Final diff inspection | `git diff --check` | PASS, exit 0; no staged changes, dependencies/configuration/original audio unchanged. |


No existing test was deleted, renamed or weakened. Twenty-two tests were added: AE-016 eight, AE-017 five (one domain/four server), AE-018 five, AE-019 four. Three existing projection/privacy fixtures were adapted to real files with exact metadata because freshness checks now reject fictional paths; the 2,048-source count, distinct-example, owner and paging assertions remain. Projection-only dummy bytes are not claimed as decoder evidence; extraction/HTTP tests use real encoded FLAC/WAV or generated valid WAV fixtures.

New AE-016 tests in declared `ae016_tests.rs`: `hot_size_replacement_invalidates_descriptor_and_paging_without_rescan`, `hot_nanosecond_change_invalidates_descriptor_and_paging_without_rescan`, `hot_missing_source_invalidates_descriptor_and_paging_without_rescan`, `cold_size_replacement_cannot_warm_a_stale_cached_descriptor`, `cold_nanosecond_change_cannot_warm_a_stale_cached_descriptor`, `cold_missing_source_cannot_warm_a_stale_cached_descriptor`, `warming_revalidates_an_unchanged_catalogue_and_can_reuse_a_restored_original`, `unchanged_real_source_reuses_persisted_features_and_preserves_paging_and_bytes`.

New AE-017 tests: domain `measured_recipe_ids_cannot_be_used_by_semantic_rules`; server `measured_recipe_ids_are_refused_by_mood_put_without_changing_any_owner`, `preference_patches_cannot_replace_measured_recipes_or_change_any_owner_on_refusal`, `mood_preference_json_without_reserved_rule_ids_remains_untyped`, `persisted_reserved_moods_are_reported_without_replacing_recipes_or_losing_preferences`.

New AE-018 tests in declared `audio_features/worker_tests.rs`: `disjoint_catalogues_bound_resident_facts_and_preserve_reusable_durable_records`, `unavailable_replacement_catalogue_does_not_keep_historical_resident_facts`, `catalogue_reload_while_worker_waits_cannot_publish_obsolete_cached_sources_or_epoch`, `cancelled_queued_extraction_keeps_completed_cache_reusable_after_restart`, `a_supervised_worker_keeps_the_shared_catalogue_as_its_resident_fact_boundary`.

New AE-019 tests in declared `audio_features_budget_tests.rs`: `eof_read_cannot_publish_features_after_exceeding_the_active_budget`, `eof_read_counts_toward_the_total_active_budget`, `eof_at_the_budget_boundary_and_long_checkpoint_waits_remain_valid`, `pcm_read_still_refuses_an_exceeded_active_budget`. Timing and packet state are local to each test; native callers and controlled fixtures use the same measurement loop. Real decoder/OS cancellation latency remains outside this proof.

Earlier read-only discovery: 35 existing tests PASS, real-WAV HTTP probe reproduces AE-016/AE-017, exact-function clock probe supports AE-019, and static independent inspection supports AE-018. These do not count as independent reviews of MOODS-C1. Exploratory source/command manifest SHA-256 `8c433d39d1f4b7c602df6c9e7f52919cee3c4cae4bf8a4b8f0f6df2a61c3b216`; HTTP log SHA-256 `c9b84ed740f3e834506d4166f29a3e11f34e4ed53e4431e24a76283dfe0c729d`; controlled-budget log SHA-256 `cc83fba568df4c72f0180777c5c355a00e0bd351c484fc7e5a06d00d1790a3e6`. The runtime retention failures above are new corrective-phase evidence, not a rewritten claim about the static audit.

Ephemeral parent logs: `aede-ae019-before.log`, `aede-ae016-before.log`, `aede-ae017-{domain,server,legacy}-before.log`, `aede-ae018-before.log`, `aede-moods-{core,server,domain}-after.log`. Retained source assertions and exact commands remain the durable evidence. Final checks are in the table above. MOODS-C1 software fingerprint remains `a2152c8a275ad452a9f5b9db3c586e009f4d94c5122aa9b2c3c2ef55fc20d916` across 173 files; the final diff/index checks found only the authorized 16 Rust and four Markdown files. No original test identity was lost. These correction-stage runs alone do not establish the complete root gate or independent review; R-MOOD-01 and later parent gate execution are recorded separately below. Other-platform/MSRV, advisory lookup and physical/client/NAS acceptance remain unverified.

Final ephemeral log hashes (SHA-256): `aede-moods-core-after.log` = `d365d395f91dbae86a9f74520f246ace3423634078528aefaf861e8e5562792b`; `aede-moods-domain-after.log` = `4ca03bd24b625e84b7a4da3169f607c35902342b9bc0d94ff4e00c831aeb9228`; `aede-moods-server-after.log` = `fb370ff5b06cf875c7ec68f4a468917f10c0b10bc865a3a4eda08af3a4490038`; `aede-moods-server-final.log` = `57ffabbdd0ccb1b8bf8300f6d8376df8424606dbe83f824ce3921b860a4651bc`; `aede-moods-clippy.log` = `6eee291cb054723ae203090249677f9b9d29333620808a552acb71f4ea3a4e35`. Markdown-only final recording does not change the candidate's Rust fingerprint.

### Moods independent review and acceptance

- The maintainer accepted AE-016–AE-019 and requested recording [R-MOOD-01](findings.md#r-mood-01--independent-moods-correction-review), its limits, complete root verification and local commits after final diff checks, without push. Rust software remains unchanged at the 173-file MOODS-C1 fingerprint; only quality Markdown changes in this recording phase.
- Fresh-context `moods_c1_independent_review` is independent of discovery/source/test authors and earlier reviewers. All four findings are CONFIRMED with corrections supported, unchanged severity/confidence and no new confirmed defect. It is one qualifying affected-subset review, not a second current-candidate or whole-campaign review.
- Reviewer execution: the exact current server binary ran 13 neighboring owner/auditor/parental tests PASS; current core binary ran 15 PCM stream tests PASS; locked/offline core build PASS and an eight-scenario stdin probe PASS. Exact commands, identities, source/binary hashes and scope are in the register. Old before/after executables were not rerun; their logs were inspected as historical evidence.
- Parent execution during review: 14 moods, nine core extraction/cache, 40 affected server and five documentation tests PASS, none failed/ignored; formatting/diff checks and all changed/untracked file hashes matched before/after. These 68 executions are parent results, separate from the reviewer's 28 tests and probe; no fresh log files were written for that parent rerun.
- Limit 1: queued-reload coverage ignores the worker outcome; precise terminal status/failure counters remain unasserted. Limit 2: cancellation occurs before the worker lock and resumes in the same state, with fresh-state restart covered separately; cancellation during actual decoding/real Stop latency remains unverified. Limit 3: durable budget cases do not cover multiple PCM packets or transformation-time clock advance; the temporary independent probe supplies complementary evidence. These are strengthening follow-ups, not new product defects or grounds to waive unrelated controls.
- Existing source identity/TOCTOU, native/MSRV, client/NAS metadata/memory/performance, exhaustive I/O/interleavings and physical/provider/renderer acceptance remain limits. Reviewer and parent targeted checks do not substitute for the complete root gate below or AE-C02–AE-C13.

### AE-C01 — MOODS-C1 final root gate

- Authorization/candidate: after accepted R-MOOD-01, the maintainer requested complete verification before local commits. Exact parent command `AEDE_REQUIRE_FFMPEG=1 bash tools/check.sh`, repository root, 2026-10-22, macOS ARM64, Rust/Cargo 1.99.0 Homebrew and installed FFmpeg, with authorized local sockets, pseudo-terminals and process inspection. Dependencies remained locked/offline. Base `04b6a9e9d6fa5bf1a7f5545552610628b437567a` plus frozen MOODS-C1; only quality-record Markdown changes during this phase.
- Result: PASS, exit 0, ending `All green.`. Cargo unit/integration/documentation execution passed **2,802 tests** across 49 reported suites, zero failures/ignored. The six Python helper suites passed **95 tests**; the separate real-terminal helper passed **15 tests**. Later generated test-inventory enumeration is not execution and is excluded from these totals.
- Constituents: build helpers, bilingual renderer and release-version agreement, formatting, default all-target Clippy, required FFmpeg conversions, complete root Cargo tests, masked account entry, generated statistics and 116 bilingual documentation pages, local links/fragments/SEO, warning-denied Rustdoc and optimized release build all passed. Final release build: `1m 27s`.
- Frozen software: the 173-file MOODS-C1 SHA-256 `a2152c8a275ad452a9f5b9db3c586e009f4d94c5122aa9b2c3c2ef55fc20d916` matches R-MOOD-01; the 22 additions and original coverage remain. The 17 implementation/test/API-document files also match the pre-gate byte manifest. Generated `target/` and `dist-site/` output is ignored and excluded from commits; no dependency/configuration/source/test changes are introduced while recording results.
- Local commit mapping: AE-016 `81c2ec7688234519cee96cfcabe88b5f92bb0771` (freshness, eight tests and preserved fixtures), AE-017 `664bb426d0117df7460b18d8e3ed8202689a7ace` (reserved recipes, five tests and API contract), AE-018 `a763d6b7864e3c07200d7bf7da716026293449ec` (resident facts and five worker tests), AE-019 `ae7ceb0071627af0a33873a46f95a170798d16c1` (EOF budget and four tests). The combined revision matches the frozen reviewed software. The separate quality commit carries only `audit-scope.md`, `findings.md` and this checklist, including these outcome/hash entries; no generated or unrelated file is included. No push.
- Evidence: ephemeral local log `/tmp/aede-moods-c1-final-gate.log`, SHA-256 `ec7d19ed3795566b053586adddb734ff99005aa96994950ee05705d88e583294`. Raw logs are not committed; commands, sanitized results, durable tests and candidate fingerprint retain reproducible evidence. All eight quality Markdown files' local links and intra-quality anchors are checked separately; final diff/staging and post-recording documentation checks precede commits.
- Scope/limits: this is parent validation, not another independent review. AE-C02–AE-C13, supplementary/native/client/provider/hardware/NAS acceptance and the three R-MOOD-01 coverage follow-ups remain outstanding. No whole-campaign closure, release, push or automatic next audit is authorized.

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

AE009-C1 changed CLI/server code after that pair. R-AE009-01 independently reviews the affected consent delta with no new confirmed significant defect. It is one review; historical CLI-C3 reviews do not automatically become a new final-candidate pair, and the general-scope boxes remain unchecked. SERVER-C2 changes server code again; the discovery and correction agents are not qualifying post-fix reviewers, and R-SRV-01 is one independent affected-subset review. No server or general-campaign pair is recorded. ACCOUNTS-C1 changes the accounts domain afterward; its fresh-context discovery/correction/test agents are not post-fix independent reviewers. R-ACC-01 is recorded as one qualifying affected-subset review; no accounts/general-campaign pair has been established. MOODS-C1 changes core/moods/server code afterward. Its discovery/correction agents and the parent are not independent post-fix reviewers; R-MOOD-01 is now recorded as its first qualifying affected-subset review, with no second current-candidate or general-campaign pair.

## 7. Limits and scope accounting

- [ ] The final report lists actually inspected paths, uninspected areas, exclusions and confidence limits.
- [ ] All applicable mandatory software controls have evidence-backed `PASS` results.
- [ ] Unavailable physical, platform-interactive, provider/client and NAS acceptance is explicitly recorded without claiming success.
- [ ] Whole-process resource, shared-budget, trusted-operator and filesystem TOCTOU boundaries are retained.
- [ ] The maintainer accepts documented residual risks and campaign limitations; any exceptional High waiver is separately identified.

## 8. Campaign decision and final report

Current recommendation: `CONTINUE` — MOODS-C1 contains the accepted AE-016–AE-019 corrections, uncommitted when reviewed and now locally committed as `ae7ceb0071627af0a33873a46f95a170798d16c1`. Before-fix failures, retained tests and targeted checks are recorded above. R-MOOD-01 independently supports the corrections, with three coverage limits retained. The maintainer accepted it and authorized complete current-candidate root validation and local commits after final diff checks; complete AE-C01 now PASS on unchanged MOODS-C1. Earlier ACCOUNTS-C1/SERVER-C2/core/CLI evidence does not validate this later delta. The four local correction/test/API-document commits followed complete gate/final diff/staging verification, with the quality records packaged separately; no campaign closure, push or release is authorized.

At decision time select exactly one of `CONTINUE`, `FIX AND REVALIDATE` or `CLOSE CAMPAIGN` under the [canonical policy](README.md#campaign-verdict-and-release-authorization).

- Reviewed revision and actual scope: current moods baseline `04b6a9e9d6fa5bf1a7f5545552610628b437567a` plus MOODS-C1; all mood-domain modules and relevant native extraction/cache, shared/private projection, preference, worker/reload/cancellation boundaries at the recorded depth. Earlier candidates and reviewed scopes remain explicitly historical above.
- Commands actually executed and results: current before-fix commands FAIL on each accepted defect with positive controls retained; after-fix moods/core/affected-server commands PASS as recorded above. Complete default server library (429), targeted Clippy/Rustdoc, final formatting/diff inspection and five documentation tests PASS; exact commands are recorded above. Complete current-candidate AE-C01 also PASS: 2,802 Rust tests across 49 suites, none failed/ignored, plus 95 helper tests, 15 terminal tests, site/Rustdoc and optimized build. The all-feature/platform/desktop matrix and fresh advisory checks remain NOT RUN.
- New / fixed and verified / rejected / deferred findings: current accepted AE-016–AE-019 are VERIFIED on MOODS-C1, with before/after evidence and exact lifecycle in the register. No waiver, deferral or new finding beyond those four. AE-001–AE-015 retain their prior verification; a historical complete gate is not current delta evidence.
- Qualifying independent review references: R-MOOD-01 for the four-finding moods subset, with its three coverage limits. R-ACC-01, R-SRV-01, R-AE009-01, R-CLI-03/R-CLI-04 and R-CORE-01 remain attributed to their historical candidates; no new final-candidate or general-campaign pair is established.
- Exceptional unresolved High waivers: none recorded.
- Failed or blocked mandatory controls: AE-C01 PASS on MOODS-C1; AE-C02–AE-C13 remain NOT RUN and prevent general closure. Historical accounts/server gates remain recorded separately. Native/MSRV, independent desktop and fresh advisory evidence remain essential software gaps.
- Residual risks and uninspected areas: source identity/TOCTOU, packet-admission/real Stop latency, target-NAS metadata/memory/performance, display of legacy-conflict diagnostics, a second current-candidate independent review and the remaining mandatory controls; R-SRV-01's snapshot-rendezvous and blocked-publication admission-test gaps; R-AE009-01's durable-test/harness, populated-store and mixed-version gaps; remaining final-candidate reviews/native-platform validation; unverified additional raw styled-input callers; exhaustive option combinations, filesystem races, late restore I/O/process interleavings, full playback/device/desktop internals, platform/hardware/provider/client/NAS acceptance. The general campaign extends beyond the inspected core persistence, CLI and first server paths. Direct scan-handler cancellation has no completed real-disconnect reproduction; it remains a limit, not a fifth confirmed server defect.
- Historical validator recommendation: `cli_validator`, fresh read-only context with no source/test authorship, independently checked CLI-C3, gate hash/results, finding histories, review reset/pair and documentation on 2026-10-22. It recommended `CONTINUE` for general closure and supported the two then-authorized local commits. R-ACC-01 is the earlier accounts independent review; no current independent validator has run. Historical local AE-009 commit authorization comes from the maintainer's subsequent explicit acceptance, not that historical recommendation. The later server local-commit authorization comes directly from maintainer acceptance after R-SRV-01, subject to the requested gate/diff verification. No release authority granted.
- Maintainer campaign decision and date: not yet made.

## 9. Separate release authorization

Release authorization: **not granted**. `CLOSE CAMPAIGN` would close only the agreed audit scope and would not authorize tagging, publishing or distributing a release.

Before a release decision, record the selected version/tag/commit, native target artifacts and checksums, release workflow evidence, package contents (including shared crates), applicable client/hardware/deployment acceptance, dependency evidence, unresolved findings and release-specific risk decisions. A campaign High waiver does not itself approve release of that defect. An unresolved Critical prevents release approval.

- Release candidate/version/targets: not selected.
- Applicable acceptance and packaging evidence: not available for this campaign.
- Release blockers/limits: unevaluated candidate; outstanding platform artifacts and separate acceptance activities above.
- Maintainer release decision, rationale and date: not yet made.

Neither this checklist nor a clean review certifies the absence of bugs.
