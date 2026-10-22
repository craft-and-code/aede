# Aède Quality Assurance

## Purpose and document map

This directory defines Aède's stabilization process. It supports evidence-based decisions; an audit does not prove that software is free of bugs.

| Document | Responsibility |
| --- | --- |
| This README | Canonical severity, evidence, review and closure rules |
| [Audit scope](audit-scope.md) | Campaign identity, authorized phase, included paths, validation commands and limits |
| [Findings](findings.md) | Stable finding records, dispositions and independent review records |
| [Release and stabilization checklist](release-checklist.md) | Actual validation results, final campaign report and separate release decision |
| [Auditor](prompts/auditor.md) | Read-only investigation of execution paths and defects |
| [Test engineer](prompts/test-engineer.md) | Behavioral coverage review and regression work within current permissions |
| [Adversarial reviewer](prompts/adversarial-reviewer.md) | Independent examination of findings, fixes and missed failure modes |
| [Release validator](prompts/release-validator.md) | Evidence consolidation and campaign recommendation |

Follow the root [AGENTS.md](../../AGENTS.md), [CLAUDE.md](../../CLAUDE.md), [current state](../coding/current-state.md), relevant design documents and [engineering rules](../coding/engineering-rules.md). Desktop work also follows [desktop instructions](../../desktop/AGENTS.md). Repository-facing records and all four prompts use English.

## Authorization and workflow

A scope or command listed here is not authorization to execute it. The current maintainer instruction and the authorized phase in the scope govern every role. A documentation-preparation pass is not a code audit or an independent review.

1. Record the branch, starting commit and working-tree state; preserve unrelated changes.
2. Agree the scope, required controls, acceptance limits and current permissions.
3. Run the first code audit in read-only mode only after that phase is authorized.
4. Triage hypotheses, verify evidence and deduplicate findings before proposing fixes.
5. When separately authorized, make small, reviewable corrections with behavioral regression coverage. Do not automatically fix every reported issue.
6. Execute the applicable controls only when execution is authorized, retaining exact commands and results.
7. Obtain independent reviews and revalidate affected paths after changes.
8. Evaluate the checklist and record the campaign recommendation and maintainer decision.
9. Evaluate release readiness separately if a release is requested.

Auditors and adversarial reviewers do not modify files. The test engineer may implement or execute tests only within explicit task permissions. The validator recommends a decision and does not approve a release. Do not create commits without explicit authorization or coordinate concurrent writes to the same files without an agreed owner.

## Finding identity, severity and confidence

Use stable `AE-*` identifiers, allocated sequentially starting with `AE-001`. Reuse the identifier for the same underlying issue across reviews and campaigns; never create a duplicate merely because its wording changes. Keep useful rejected findings and their rationale.

Separate confirmed defects, unverified hypotheses and unresolved risks in the register. A missing test is not automatically a defect. High confidence is not a substitute for confirmation.

| Severity | Meaning |
| --- | --- |
| Critical | Severe security compromise, major data loss/corruption or another catastrophic failure |
| High | Serious functional, security, reliability or data-integrity defect |
| Medium | Meaningful defect with limited impact or a reasonable workaround |
| Low | Minor defect with limited consequences |

Severity reflects actual impact and exploitability. A **significant defect** is a confirmed Critical, High or Medium defect. An unresolved hypothesis of a Critical, High or Medium defect must be investigated or explicitly dispositioned with evidence; it must not disappear from the report merely because it is unconfirmed.

Record confidence as `HIGH`, `MEDIUM` or `LOW`, with a rationale describing what is established and what remains uncertain. An unverified severity is provisional.

## Evidence and results

Use the detailed record fields and lifecycle statuses in [findings.md](findings.md). Every record must preserve the relevant revision and file locations, triggering scenario, expected and actual behavior, impact, severity, confidence, supporting evidence, missing verification and required regression tests.

Label behavior inferred from a static execution path as such; do not describe it as runtime observation. Never claim that a command, test, build or manual check succeeded without its execution result. Historical verification in other documents remains historical and does not validate the current candidate.

Record exact commands, working directory, revision and relevant dirty state, toolchain, platform, prerequisites, date, exit status and sanitized output or an accessible evidence link. Avoid personal paths, identifiers, credentials and private library contents.

Validation results use only:

- `PASS`: executed successfully, with evidence;
- `FAIL`: executed and failed, with evidence;
- `BLOCKED`: execution could not complete because a documented prerequisite or environment was unavailable;
- `NOT RUN`: not executed;
- `NOT APPLICABLE`: outside the agreed applicability, with a justification.

Every applicable mandatory control must reach `PASS`. A `BLOCKED` or `NOT RUN` mandatory software control prevents closure; it cannot be relabeled as a hardware limitation. Existing dependency-policy exceptions must remain visible in the evidence and do not justify expanding an allowlist.

## Independent reviews

Record each completed review in the register with its reviewer or review context, distinct focus, revision, actual coverage, evidence, new finding IDs and limitations. Independence requires direct examination of code and evidence separately from the implementation and preceding review; restating a prior report does not qualify.

The two consecutive qualifying reviews must cover the agreed scope on the final candidate without any new confirmed significant defect. Newly confirming a previously recorded Critical, High or Medium hypothesis also breaks that sequence. A substantive candidate change requires affected paths to be reviewed and revalidated before earlier review evidence can be relied on; record how both reviews remain applicable to the final candidate.

Documentation preparation, test runs and historical milestone reviews do not count as these campaign reviews. Low findings must still be triaged even when they do not reset the significant-defect sequence.

## Campaign closure policy

All of the following are required:

- No unresolved Critical defect remains. There is no Critical waiver.
- No unresolved High defect remains for normal closure. An exceptional waiver requires explicit maintainer acceptance recorded against each finding, including impact, rationale, compensating measures, owner, follow-up and review trigger. It remains visibly unresolved and the closure report must identify the exception.
- Medium and Low defects are qualified and either resolved with appropriate verification or deferred with a documented rationale, owner and follow-up.
- All findings have an evidence-based status; significant fixes have suitable regression coverage and affected behavior has been revalidated.
- Two consecutive independent reviews reveal no new confirmed significant defect under the review rules above.
- Every applicable mandatory validation control passes on the reviewed candidate.
- Actual coverage, uninspected areas, residual risks and unavailable hardware/environment acceptance are documented and accepted by the maintainer.

Unavailable physical or deployment acceptance may remain an explicit campaign limitation when the agreed scope makes it supplementary. It must not be reported as passed, and it does not waive a mandatory software control.

Reopen the relevant scope when a significant new defect is confirmed and repeat the necessary reviews and controls. Closing a campaign does not end normal regression testing or focused reviews after later changes.

## Campaign verdict and release authorization

Use exactly one campaign verdict:

- `CONTINUE`: investigation, reviews or essential evidence are incomplete.
- `FIX AND REVALIDATE`: confirmed defects or failed checks require corrective work and verification.
- `CLOSE CAMPAIGN`: every campaign closure condition is satisfied, with any exceptional High waiver explicitly identified.

Record the final report in the checklist: reviewed revision and scope, checks actually executed, findings created/fixed/rejected/deferred, review evidence, unresolved risks, recommendation and maintainer decision.

`CLOSE CAMPAIGN` closes only the agreed audit scope. It does not authorize a tag, build publication or release. Release approval requires a separate maintainer decision on a specific candidate, supported by its platform artifacts, packaging and applicable hardware, client and deployment acceptance. The validator must state any release blockers independently of its campaign verdict.
