# Audit Scope

## Campaign information

- Project: `REPLACE_WITH_PROJECT_NAME`
- Campaign ID: `REPLACE_WITH_CAMPAIGN_ID`
- Starting commit: `REPLACE_WITH_GIT_COMMIT`
- Branch: `REPLACE_WITH_BRANCH`
- Campaign type: `STABILIZATION`
- Date: `YYYY-MM-DD`
- Responsible maintainer: `PROJECT_MAINTAINER`

## Campaign objectives

Describe the reason for this audit.

Examples:
- General stabilization before a release.
- Investigation of a reported defect.
- Review of a security-sensitive component.
- Verification of recent architectural changes.
- Regression assessment after a substantial refactoring.

Objectives:

- `REPLACE_WITH_OBJECTIVE_1`
- `REPLACE_WITH_OBJECTIVE_2`

## Scope

### Included

List the components, directories, modules, APIs, and execution paths to inspect.

- `REPLACE_WITH_PATH_OR_COMPONENT`
- `REPLACE_WITH_PATH_OR_COMPONENT`

### Excluded

List components intentionally excluded from this campaign and explain why.

- `REPLACE_WITH_PATH_OR_COMPONENT_AND_REASON`

An excluded component must not be described as audited.

## High-risk areas

Identify the risks relevant to this project.

Consider:
- data integrity and persistence;
- error handling and recovery;
- authentication and authorization, where applicable;
- input validation and external data;
- concurrency and resource lifecycle;
- API behavior and compatibility;
- filesystem access and permissions;
- external dependencies and network operations;
- migrations and backward compatibility;
- performance and resource exhaustion;
- installation, upgrades, and release packaging.

Select only areas relevant to the actual architecture.

## Recent changes

Record the changes most likely to introduce regressions.

- Changed modules:
- New features:
- Refactored components:
- Dependency updates:
- Database or file format changes:
- API changes:
- Known incidents:

## Existing tests and controls

Record the commands appropriate to this repository.

- Format check:
- Build or compilation:
- Unit tests:
- Integration tests:
- Static analysis or lint:
- Security checks:
- Other required checks:

Commands must be taken from the actual project configuration or verified documentation.

Do not invent commands or assume that a tool is installed.

## Known findings

List relevant findings from `findings.md`.

- Existing finding IDs:
- Previously rejected findings relevant to this scope:
- Deferred findings:
- Known limitations:

## Audit instructions

The Code Auditor must:
1. Read the project instructions and relevant source code.
2. Inspect the included execution paths.
3. Follow important data flows and error paths.
4. Verify existing findings before creating new ones.
5. Distinguish confirmed defects from hypotheses.
6. Record files and locations that support each finding.
7. Report uninspected areas and environmental limitations.

The Test Engineer must:
1. Identify the behaviors that need protection.
2. Review existing regression coverage.
3. Propose tests that would fail when the defect is present.
4. Avoid redundant tests without a clear benefit.

The Adversarial Reviewer must:
1. Independently examine the relevant code.
2. Challenge significant findings and claimed fixes.
3. Investigate related failure modes.
4. Identify meaningful gaps in the audit.

## Completion criteria

The campaign is complete when:
- the defined scope has been inspected to the agreed extent;
- important findings have been verified and classified;
- required corrections and regression checks have been completed;
- the release checklist has been evaluated;
- remaining risks and uninspected areas are documented.

A narrow audit must not be presented as a full repository audit.

## Campaign outcome

- Result: `NOT STARTED`
- Findings created:
- Findings fixed:
- Findings rejected:
- Findings deferred:
- Checks passed:
- Checks failed:
- Checks not executed:
- Remaining risks:
- Recommended next action: