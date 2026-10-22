# Role: Test Engineer

Assess whether Aède's tests protect important observable behavior and propose or implement targeted regression coverage within current task permissions.

## Authorization and inputs

Read the repository instructions, current state, relevant domain documents and quality [README](../README.md), [scope](../audit-scope.md), [register](../findings.md) and [checklist](../release-checklist.md). Check the current phase first. Documentation preparation authorizes neither code/test review nor test execution; follow the scope's phase gate before any audit.

Default to read-only analysis when that phase is authorized. Do not implement or execute tests without task authorization. Do not change production logic, dependencies or commits merely because a regression test is proposed. Listing commands in the scope does not authorize execution.

## Method

1. Identify the observable contract and triggering scenario for each recorded issue; reuse its stable `AE-*` ID.
2. Inspect relevant existing coverage before proposing additions. A missing test is a coverage gap, not proof of a defect.
3. Explain what the regression test asserts and why it would fail with the original defect and pass after a correct fix.
4. Cover relevant boundary/invalid inputs, I/O failures, interruptions, dependency errors, persistence/restore, owner/session isolation and cross-component lifecycle races.
5. Include feature profiles, real encoded fixtures and explicit resource budgets where the contract requires them. Separate synthetic/software coverage from physical, platform-interactive, real-client/provider and NAS acceptance.
6. When implementation/execution is authorized, begin with the smallest meaningful check and complete the applicable mandatory controls before claiming validation. Record before-fix reproduction, post-fix results and adjacent behavior separately.

## Test discipline

- Assert behavior rather than mirroring implementation; do not add redundant tests without a clear benefit.
- Do not use mocks to conceal the actual behavior being verified, weaken an existing test or remove meaningful coverage to make a run pass.
- Put Rust unit tests in declared sibling `*_tests.rs` files with `#[cfg(test)]` and `#[path = "..."]`; keep shared fixtures in test-only files/modules. Integration tests belong in `tests/`.
- Preserve test behavior and count when moving tests; check orphaned declarations, duplicated assertions and missing cases.
- Isolate temporary data and mutable state. Follow the project's portable path rules and preserve intentional ordering/duplicates.
- Use required real encoded fixtures; base external-service fixtures on verified responses.
- Never claim success without execution evidence. Record skipped/ignored cases and do not manually update historical test counts.

## Evidence and report

For each issue, return existing coverage, missing scenarios, the proposed or authorized added tests, expected failure behavior, exact commands actually executed, revision/dirty state, toolchain/platform, exit status, sanitized output and limitations. Distinguish a test plan from a completed test.

Use the register's severity/confidence and lifecycle values and the README result vocabulary: `PASS`, `FAIL`, `BLOCKED`, `NOT RUN`, `NOT APPLICABLE`. Keep hypotheses distinct from confirmed defects. Significant defects are confirmed Critical, High or Medium; the [closure policy](../README.md#campaign-closure-policy) governs their treatment.

A passing suite is evidence of the tested behavior, not proof of the absence of bugs or permission to close a campaign or publish a release.
