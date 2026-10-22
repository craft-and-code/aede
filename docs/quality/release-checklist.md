# AE-STAB-001 — Stabilization and Release Checklist

## Campaign and candidate information

- Project/campaign: Aède / `AE-STAB-001`.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- Preparation date: `2026-10-22`.
- Initial code-review target: the starting commit; final reviewed candidate is not selected.
- Responsible authority: Aède maintainer; decision attribution remains to be recorded.
- Release version, candidate tag and publication targets: not selected.
- Current phase: documentation preparation; first code audit `NOT STARTED`, pending maintainer validation.

The [scope](audit-scope.md) defines commands/applicability and current authorization. The [README](README.md) defines closure policy and evidence vocabulary. This checklist records results and the final report; listing a check does not authorize its execution.

## 1. Source control and change review

Preparation confirmed `main` at the starting commit with a clean working tree before these documentation edits. This observation is not verification of a later code/release candidate.

- [ ] The final candidate revision and relevant dirty state are recorded.
- [ ] Branch and working-tree state were checked before each authorized mutation.
- [ ] Unrelated changes were preserved and all final changes are within the authorized scope.
- [ ] Intended changes were reviewed and remain traceable in Git.

## 2. Findings and risk assessment

The [register](findings.md) currently has no findings; the code audit has not started.

- [ ] No unresolved Critical defect remains; no Critical waiver is permitted.
- [ ] No unresolved High defect remains for normal closure, or every exceptional waiver has explicit maintainer acceptance and the full rationale, compensating measures, owner, follow-up and review trigger in its finding record.
- [ ] Medium and Low defects are qualified and either resolved with verification or deferred with rationale, owner and follow-up.
- [ ] Hypotheses, confirmed defects and material unresolved risks are distinguished; important uncertainty has an evidence-based disposition.
- [ ] Duplicates are consolidated and useful rejection evidence is retained.
- [ ] Residual risks and exceptional unresolved High findings are visible in the final report.

## 3. Validation results

All application controls below are `NOT RUN`. No tests, builds, lints or advisory lookups were executed during documentation preparation. Historical results in current-state/milestone documents are not campaign passes.

Commands are defined once in [scope validation controls](audit-scope.md#validation-controls-and-evidence). Record the exact executed command, working directory, revision/dirty state, toolchain/platform, date, exit status and sanitized output or accessible evidence link for each result. Aggregate gate logs may substantiate their constituent checks without implying additional executions.

| ID | Mandatory software control | Result | Execution evidence |
| --- | --- | --- | --- |
| AE-C01 | Complete root gate: format, lint, tests with FFmpeg, helpers, terminal, versions, site, Rustdoc and optimized build | NOT RUN | None |
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

Review records: none. No preparation activity or historical milestone review counts toward the pair.

## 7. Limits and scope accounting

- [ ] The final report lists actually inspected paths, uninspected areas, exclusions and confidence limits.
- [ ] All applicable mandatory software controls have evidence-backed `PASS` results.
- [ ] Unavailable physical, platform-interactive, provider/client and NAS acceptance is explicitly recorded without claiming success.
- [ ] Whole-process resource, shared-budget, trusted-operator and filesystem TOCTOU boundaries are retained.
- [ ] The maintainer accepts documented residual risks and campaign limitations; any exceptional High waiver is separately identified.

## 8. Campaign decision and final report

Current recommendation: `CONTINUE` — documentation awaits validation; the first audit, required reviews and software controls have not run. Campaign closure has not been approved.

At decision time select exactly one of `CONTINUE`, `FIX AND REVALIDATE` or `CLOSE CAMPAIGN` under the [canonical policy](README.md#campaign-verdict-and-release-authorization).

- Final reviewed revision and actual scope: not yet established.
- Commands actually executed and results: no software validation executed; documentary consistency and Git-diff checks only.
- New / fixed and verified / rejected / deferred findings: none recorded yet; no code audit performed.
- Qualifying independent review references: none.
- Exceptional unresolved High waivers: none recorded.
- Failed or blocked mandatory controls: none executed; all remain `NOT RUN` and prevent closure.
- Residual risks and uninspected areas: all code remains unaudited in this campaign; see scope exclusions/limits.
- Validator recommendation and supporting rationale: `CONTINUE` for the reasons above.
- Maintainer campaign decision and date: not yet made.

## 9. Separate release authorization

Release authorization: **not granted**. `CLOSE CAMPAIGN` would close only the agreed audit scope and would not authorize tagging, publishing or distributing a release.

Before a release decision, record the selected version/tag/commit, native target artifacts and checksums, release workflow evidence, package contents (including shared crates), applicable client/hardware/deployment acceptance, dependency evidence, unresolved findings and release-specific risk decisions. A campaign High waiver does not itself approve release of that defect. An unresolved Critical prevents release approval.

- Release candidate/version/targets: not selected.
- Applicable acceptance and packaging evidence: not available for this campaign.
- Release blockers/limits: unevaluated candidate; outstanding platform artifacts and separate acceptance activities above.
- Maintainer release decision, rationale and date: not yet made.

Neither this checklist nor a clean review certifies the absence of bugs.
