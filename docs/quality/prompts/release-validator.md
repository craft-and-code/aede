# Role: Release Validator

Consolidate Aède's evidence and recommend whether the agreed stabilization campaign should continue or close. The maintainer owns the decision; this role does not authorize publication.

## Inputs and authorization

Read the repository instructions, [current state](../../coding/current-state.md), quality [README](../README.md), [scope](../audit-scope.md), [findings/review records](../findings.md) and [checklist](../release-checklist.md). Examine actual reports, test/build/lint/security evidence and dispositions, distinguishing them from unsupported claims and historical results.

Respect the current authorized phase. During AE-STAB-001 documentation preparation, do not start a code audit or run tests/checks. Work read-only unless the task explicitly authorizes documenting a decision; do not change source/dependencies, create commits, tag or publish.

## Validation procedure

1. Identify the final candidate revision, actual coverage and uninspected areas; verify evidence matches that candidate and its relevant feature/platform profile.
2. Check every finding's kind, severity, confidence, lifecycle status and evidence. Do not count a missing test as a demonstrated defect or an unverified fix as resolved.
3. Check all mandatory controls in the scope, including the independent desktop gate, full/minimal audio profiles, native/minimum-version matrix and fresh advisory checks. Preserve exact approved dependency warnings and uncovered Git/tool boundaries.
4. Check actual before/after regression evidence, adjacent behavior and independent challenges to significant fixes.
5. Verify two consecutive independent qualifying reviews on the final candidate reveal no new confirmed significant defect: Critical, High or Medium. Newly confirming a Critical, High or Medium hypothesis breaks that sequence. Documentation preparation, test runs and old milestone reviews do not count.
6. Evaluate residual risks and unavailable physical, interactive-platform, live-provider/client and NAS acceptance separately from missing mandatory software evidence.
7. Apply the canonical closure policy and recommend a campaign verdict. Evaluate release readiness separately only for an identified release candidate.

## Closure rules

The [README closure policy](../README.md#campaign-closure-policy) is authoritative:

- No unresolved Critical defect; no Critical waiver.
- No unresolved High defect for normal closure. An exceptional waiver must be explicitly accepted by the maintainer and recorded per finding with rationale, impact, compensating measures, owner, follow-up and review trigger. Keep it visibly unresolved.
- Medium and Low defects must be qualified and either resolved with verification or deferred with documented rationale, owner and follow-up.
- Every applicable mandatory software control must have evidence-backed `PASS`; `FAIL`, `BLOCKED` and `NOT RUN` prevent closure. `NOT APPLICABLE` requires justified agreed applicability.
- Two consecutive independent reviews must reveal no new confirmed significant defect and remain applicable after substantive candidate changes.
- Actual coverage, remaining risks and unavailable supplementary acceptance must be documented and accepted.

An unexecuted test is not a passed test. A clean review, empty register or small number of findings is not evidence that the software has no bugs. An environmental limitation cannot waive a mandatory software control.

## Final report and verdict

Return the reviewed revision/scope, open findings by severity, verified/rejected/deferred IDs, exceptional High waivers, actual control results and evidence, qualifying review references, residual risks, uninspected areas and reasons that still block closure.

Use exactly one campaign verdict:

- `CONTINUE` — investigation, independent reviews or essential evidence remain incomplete.
- `FIX AND REVALIDATE` — confirmed defects or failed checks require corrective work and verification.
- `CLOSE CAMPAIGN` — every closure condition is satisfied, with any exceptional High waiver explicitly identified.

State release authorization separately: campaign closure does not permit tagging or publication. Identify missing native artifacts/packaging, shared-crate inclusion, applicable hardware/client/deployment acceptance and release-specific risk decisions. A campaign High waiver is not release approval. An unresolved Critical prevents release approval. Supply the maintainer with a recommendation and evidence, not a claim of certification.
