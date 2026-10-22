# Aède Findings Register

## Current campaign

- Campaign: `AE-STAB-001`.
- Branch: `main`.
- Starting commit: `2ecadf767129ea1710bc38d4e67db43c3d50a1cc`.
- Preparation date: `2026-10-22`.
- Scope and current permissions: [audit-scope.md](audit-scope.md).
- Severity, confidence, evidence and closure rules: [README.md](README.md).
- First code audit: `NOT STARTED`; documentation awaits maintainer validation.

No findings are recorded. The former fictitious examples have been removed; they were not Aède defects or verification evidence. An empty register is not evidence that the software has no defects.

## Finding index

Allocate stable `AE-*` identifiers sequentially; the next available identifier is `AE-001`. Allocate an ID only to a real recorded defect, hypothesis or material unresolved risk. Reuse it across reviews/campaigns and link duplicates to the existing record.

| ID | Kind | Severity | Confidence | Status | Summary | Detailed record / evidence |
| --- | --- | --- | --- | --- | --- | --- |

There are no index entries or detailed findings yet. The field specification below is not an example finding.

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

No campaign review has taken place. Documentation preparation and historical milestone reviews do not count toward the required consecutive pair.

For each completed review, add a record containing reviewer/context and independence rationale, date, candidate revision/dirty state, distinct focus, actual coverage, evidence examined, challenged findings and verdicts, newly confirmed significant IDs, other new IDs, limitations and required follow-up. Per-finding review conclusions use `CONFIRMED`, `REFUTED`, `PARTIALLY CONFIRMED` or `NOT VERIFIABLE`; these are not campaign verdicts.

| Review reference | Revision | Reviewer/context and focus | New confirmed significant IDs | Coverage / evidence / limitations |
| --- | --- | --- | --- | --- |

A significant defect is a confirmed Critical, High or Medium defect. Record whether each review qualifies under the [independence rules](README.md#independent-reviews). Newly confirmed significant defects reset the consecutive clean-review sequence; retain earlier records. Explain the applicability of both qualifying reviews to the final candidate after any substantive changes.

## Campaign history

No completed campaign exists in this register. At closure, retain the starting/final revisions, actual scope, new/verified/rejected/deferred IDs, duplicate decisions, executed controls and evidence, two qualifying reviews, remaining risks/acceptance limits and maintainer decision. The final campaign report and separate release decision belong in the [checklist](release-checklist.md#8-campaign-decision-and-final-report).
