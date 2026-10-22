# Release and Stabilization Checklist

## Release information

- Project: `REPLACE_WITH_PROJECT_NAME`
- Version: `REPLACE_WITH_VERSION`
- Branch: `REPLACE_WITH_BRANCH`
- Candidate commit: `REPLACE_WITH_GIT_COMMIT`
- Date: `YYYY-MM-DD`
- Responsible maintainer: `PROJECT_MAINTAINER`

## 1. Source control and change review

- [ ] The target commit has been identified.
- [ ] The working tree and branch have been checked.
- [ ] No unrelated user changes have been overwritten.
- [ ] All intended changes have been reviewed.
- [ ] No unauthorized source or configuration changes remain.
- [ ] The final code changes are traceable in Git.

## 2. Findings and risk assessment

- [ ] All Critical findings have been resolved and verified.
- [ ] All High findings have been resolved and verified, or have an explicitly documented disposition accepted by the maintainer.
- [ ] Medium and Low findings have been triaged.
- [ ] Deferred findings have an owner, rationale, and follow-up plan where appropriate.
- [ ] Duplicate findings have been consolidated.
- [ ] No important finding has been silently discarded.
- [ ] Residual risks are documented.

A Critical finding blocks normal release approval.

An unresolved High finding requires an explicit risk decision and must not be treated as automatically acceptable.

## 3. Automated validation

Record the exact commands and results for this project.

| Check | Command / evidence | Result |
|---|---|---|
| Formatting | `REPLACE_WITH_COMMAND` | NOT RUN |
| Build / compilation | `REPLACE_WITH_COMMAND` | NOT RUN |
| Unit tests | `REPLACE_WITH_COMMAND` | NOT RUN |
| Integration tests | `REPLACE_WITH_COMMAND` | NOT RUN |
| Static analysis / lint | `REPLACE_WITH_COMMAND` | NOT RUN |
| Security checks | `REPLACE_WITH_COMMAND` | NOT RUN |
| Other required checks | `REPLACE_WITH_COMMAND` | NOT RUN |

Allowed results:
- `PASS`
- `FAIL`
- `BLOCKED`
- `NOT RUN`
- `NOT APPLICABLE`

Every result must be supported by actual evidence.

A check marked `NOT APPLICABLE` must have a short justification.

## 4. Regression verification

- [ ] Important confirmed defects have appropriate regression tests.
- [ ] Tests exercise the relevant behavior, not just implementation details.
- [ ] Tests fail when the original defect is reproduced, where practical.
- [ ] Tests pass after the fix.
- [ ] Relevant adjacent behavior has been checked.
- [ ] No known regression remains unexplained.

## 5. Security and data integrity

Evaluate the areas relevant to this project.

- [ ] External inputs are validated where required.
- [ ] File and database operations handle errors safely.
- [ ] Sensitive data is not unintentionally exposed.
- [ ] Access restrictions are enforced consistently, where applicable.
- [ ] Destructive operations and migrations have been reviewed.
- [ ] Recovery and backup assumptions are understood where data loss is possible.
- [ ] Dependencies and their known risks have been considered.

Mark irrelevant checks `NOT APPLICABLE` with justification.

## 6. Independent review

- [ ] The first review has been completed.
- [ ] The second review was independent and used a distinct review focus.
- [ ] New findings were deduplicated and classified.
- [ ] Significant fixes were checked for completeness.
- [ ] The two most recent independent reviews revealed no new confirmed significant defects, or the campaign remains open.

## 7. Project-specific validation

Add checks that are essential to the actual application.

- [ ] `REPLACE_WITH_PROJECT_SPECIFIC_CHECK`
- [ ] `REPLACE_WITH_PROJECT_SPECIFIC_CHECK`
- [ ] `REPLACE_WITH_PROJECT_SPECIFIC_CHECK`

## 8. Final decision

Select one result:

- [ ] `CONTINUE` — more investigation or auditing is needed.
- [ ] `FIX AND REVALIDATE` — known defects or failed checks must be addressed.
- [ ] `CLOSE CAMPAIGN` — the stabilization campaign meets its closure criteria.

### Final evidence summary

- Final commit:
- Tests and checks actually executed:
- New findings:
- Fixed and verified findings:
- Deferred findings:
- Failed or blocked checks:
- Residual risks:
- Areas not inspected:
- Maintainer decision:
- Decision date:

## Approval rule

The checklist supports a decision; it does not certify that the software is bug-free.

A campaign cannot be closed merely because few or no bugs were found.

A failed mandatory check, an unresolved Critical finding, or missing essential evidence prevents normal closure.

The maintainer must explicitly accept any documented residual risk before approving the release.