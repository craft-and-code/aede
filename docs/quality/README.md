# Quality Assurance

## Purpose

This directory contains the documentation, audit scope, defect register, release checklist, and AI agent prompts used to improve software quality.

The process applies to:
- FlacCompagnon
- Aède
- Phémios

The objective is to identify and fix meaningful defects, prevent regressions, improve test coverage, and make release decisions based on verifiable evidence.

An audit does not prove that software is free of bugs.

## Directory structure

```text
docs/quality/
├── README.md
├── audit-scope.md
├── findings.md
├── release-checklist.md
└── prompts/
    ├── auditor.md
    ├── test-engineer.md
    ├── adversarial-reviewer.md
    └── release-validator.md
```

## Roles

### Code Auditor

Investigates defects, security issues, incorrect behavior, and architectural risks.

Default mode: read-only.

### Test Engineer

Reviews existing tests, identifies missing scenarios, and proposes or implements regression tests according to the task permissions.

### Adversarial Reviewer

Independently challenges findings, checks proposed fixes, and looks for missed defects and regressions.

### Release Validator

Consolidates the available evidence and recommends whether to continue stabilization or close the campaign.

The Release Validator does not replace human approval.

## Standard workflow

1. Identify the repository, branch, and starting commit.
2. Read the repository's development rules and this quality documentation.
3. Define the audit scope in `audit-scope.md`.
4. Run the Code Auditor in read-only mode.
5. Review and classify findings.
6. Investigate confirmed findings and correct them in small, reviewable changes.
7. Add or update regression tests.
8. Run the relevant automated checks in the actual project environment.
9. Run an independent adversarial review.
10. Evaluate the release checklist and record the final decision.

Do not automatically fix every issue reported by an audit.

Confirm the defect and its impact before making changes.

## Findings management

Record confirmed defects and significant unresolved risks in `findings.md`.

Use stable, project-specific identifiers:
- `FC-001` for FlacCompagnon;
- `AE-001` for Aède;
- `PH-001` for Phémios.

Reuse an existing identifier when the same defect is reported again.

Do not create duplicate findings merely because different agents describe the same underlying issue.

Distinguish confirmed defects from unverified hypotheses.

Keep rejected findings and their justifications when they are useful for preventing repeated investigations.

## Severity levels

- **Critical:** Immediate risk of severe security compromise, major data loss or corruption, or another catastrophic failure.
- **High:** Serious functional, security, reliability, or data-integrity defect.
- **Medium:** Meaningful defect with a limited impact or a reasonable workaround.
- **Low:** Minor defect with limited consequences.

Severity must reflect actual impact and exploitability, not the apparent complexity of the code.

## Evidence requirements

Never claim that a command, test, build, or manual check succeeded unless its execution result is available.

For every significant finding, record:
- the relevant file and location;
- the triggering scenario;
- the expected and actual behavior;
- the impact and severity;
- the available evidence;
- the verification or regression test required.

A missing test is not automatically a confirmed bug.

A passing test suite is not proof of the absence of defects.

## Repository safety

Before any code modification:
- verify the current branch and working tree;
- preserve unrelated user changes;
- identify the starting commit;
- avoid destructive Git operations;
- keep changes within the authorized scope.

Audit agents must not modify source files.

Do not allow multiple agents to modify the same files concurrently without explicit coordination.

## Audit completion

A stabilization campaign may be closed when:
- no unresolved Critical or High finding remains without an explicitly accepted disposition;
- all confirmed findings have a documented status;
- required automated checks pass;
- important fixes have appropriate regression coverage;
- two consecutive independent reviews reveal no new confirmed significant defects;
- residual risks and audit limitations are documented.

A Critical or High risk cannot be ignored merely to satisfy the review-count criterion.

If a significant new defect is discovered, reopen the relevant scope and repeat the necessary checks.

## Continuous quality

Closing an audit campaign does not end quality assurance.

Use focused reviews after significant changes and the normal test suite before merging or releasing.

Reserve full repository audits for important milestones, substantial architectural changes, or release preparation.

## Final decision

Every completed campaign must produce a short report containing:
- the commit and scope reviewed;
- the checks actually executed;
- new, fixed, rejected, and deferred findings;
- unresolved risks;
- the final recommendation: `CONTINUE`, `FIX AND REVALIDATE`, or `CLOSE CAMPAIGN`.
