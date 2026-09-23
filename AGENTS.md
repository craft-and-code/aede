# Aède — Codex instructions

Follow [`CLAUDE.md`](CLAUDE.md) for the repository's project priorities, architecture, Rust conventions, verification workflow, and documentation map.
At the start of a task, read `docs/coding/current-state.md` and the relevant design or engineering documentation before changing code.

Do not enforce a maximum line length in Markdown files; write and wrap lines for readability.

## Tests

Keep tests out of production `.rs` files. Put unit tests in a sibling `*_tests.rs` file and declare that file from the module under test with `#[cfg(test)]` and `#[path = "..."]`. This preserves access to private items while keeping implementation and tests in separate files. Keep shared test fixtures in dedicated test-only modules/files, not inline in production files.

Preserve test behaviour and count when moving tests. Check for orphaned test files, duplicated assertions, and missing behavioural cases when touching a test suite.
