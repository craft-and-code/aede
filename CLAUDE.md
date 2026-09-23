# Aède — Claude Code instructions

Aède is a local-first music library engine written in Rust. It scans music folders into a persistent graph-based catalog of releases, recordings, tracks, artists, credits, relations, user annotations and external source claims.

The project is developed incrementally through the milestones defined in `docs/design/roadmap.md`.

## 1. Before changing code

At the start of a session:

1. Read `docs/coding/current-state.md`.
2. Read the relevant milestone/task documentation.
3. Inspect the existing implementation before proposing changes.
4. Follow the architecture and invariants already established in the repository.
5. Do not re-implement behaviour that already exists elsewhere.

Do not read unrelated documentation unless the current task requires it.

When a milestone is completed, the next milestone should normally start in a new Claude Code conversation.

## 2. Project priorities

These are fundamental properties of Aède:

- Local-first: no network access unless explicitly requested by a command.
- Never modify audio files or their tags.
- External metadata is stored beside local data, never over local truth.
- External claims retain their source and provenance.
- The catalog is a graph, not an album/artist hierarchy.
- Persisted data must remain compatible across versions unless an incompatible change is explicitly designed.
- Derived data must remain distinguishable from data read from files or supplied by external sources.
- Deterministic catalog construction is required.
- A command must either answer the question it was asked or refuse; it must never silently ignore an argument or option.
- User data must not be destroyed implicitly.
- Every important behavioural rule must have a test.

## 3. Architecture

### Workspace

- `crates/aede-core` — domain model, catalog, storage and library logic.
- `crates/aede-cli` — command-line interface and user-facing behaviour.
- `docs/` — architecture, design decisions, milestone plans and behavioural documentation.
- `tools/` — development and verification scripts.
- `site/` — project website.

Keep domain logic in `aede-core`. The CLI should orchestrate commands and presentation, not duplicate domain rules.

### Domain model

The model is graph-based.

- `Release` represents what users commonly call an album.
- `Recording` represents a recorded performance.
- `Track` represents the position of a recording within a release.
- Credits and relations are first-class data.
- Do not simplify the model into an album → artist hierarchy.

Domain vocabulary follows MusicBrainz terminology.

### Storage

The persisted JSON representation mirrors `schema.sql`.

- One persisted concept corresponds to one schema concept.
- Incompatible storage changes require a `FORMAT_VERSION` change.
- Adding an optional field is normally backward-compatible and does not require a format bump.
- Derived/inferred data has its own rule/versioning mechanism where required.

## 4. Rust rules

- Rust 1.89+.
- Follow the existing `rustfmt.toml` and `clippy.toml`.
- Prefer simple, readable Rust over clever abstractions.
- Minimize visibility: prefer `pub(crate)` over `pub`.
- Borrow rather than clone when practical.
- Do not introduce `unsafe` without explicit justification and discussion.
- Library code must not use `unwrap()` or `expect()`.
- Parsers must handle truncated, corrupt and malformed input without panicking.
- Never silently swallow errors.
- Comments explain why, not what.
- All repository-facing text is written in English.

See `docs/coding/engineering-rules.md` for detailed project-specific rules.

## 5. Tests

- Every bug fix starts with a failing test.
- Keep tests out of production `.rs` files. Put unit tests in sibling
  `*_tests.rs` files and declare them with `#[cfg(test)]` and `#[path = "..."]`
  so they retain access to private implementation details.
- Integration tests live in `tests/`.
- New supported audio formats require real fixtures.
- Tests must assert behaviour, not implementation details.
- Avoid process-global state in tests.
- Test names describe observable behaviour.
- Do not weaken an existing test merely to make a change pass.

Do not update test counts manually during normal development.

The current verified test count is maintained in `docs/coding/current-state.md` and should only be updated when the test suite has actually been run.

## 6. Verification

The repository's verification script is authoritative:

```sh
tools/check.sh
```

Before committing, run:

```sh
tools/check.sh
```

Useful individual commands:

```sh
cargo test
cargo fmt --all
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

When testing locally, use the smallest relevant command first. Run the complete verification before committing.

Claude Code may inspect and reason about Rust code, but if Rust execution is unavailable in the current environment, do not claim that compilation or tests passed. State clearly what was and was not verified.

## 7. Network and external services

Network access is explicit.

- Do not add automatic network access to normal catalog operations.
- External services are accessed only through the appropriate explicit command/pass.
- Respect service rate limits.
- Never overwrite local metadata with external metadata.
- Keep source attribution with externally supplied data.
- Avoid network requests when the required identifier/data is already available locally.

## 8. Dependencies

Do not add a dependency automatically.

Before proposing a new dependency:

1. Explain why the existing standard library or current dependencies are insufficient.
2. Explain what the dependency replaces or enables.
3. Consider dependency-tree size and maintenance.
4. Ask before introducing it.

Current important dependencies include:

- `lofty` — supported audio/container/tag formats where the project does not provide its own parser.
- `ureq` — optional network functionality.

## 9. CLI behaviour

The CLI is part of the public contract.

- Invalid options are errors.
- Invalid option values are errors.
- Options that a command cannot honour are errors.
- Never silently ignore user input.
- Output must remain understandable for humans.
- JSON/CSV/M3U output must remain machine-readable.
- Long-running commands should report progress and preserve completed work where appropriate.
- Destructive operations require explicit confirmation unless an explicit non-interactive confirmation option is supplied.

## 10. Documentation

Do not duplicate information unnecessarily.

Do not enforce a maximum line length in Markdown files; write and wrap lines
for readability.

- `CLAUDE.md` contains permanent instructions required during every coding session.
- `docs/coding/current-state.md` contains the current project state.
- `docs/coding/engineering-rules.md` contains detailed, stable engineering rules.
- `docs/design/roadmap.md` contains milestones and future direction.
- Topic-specific files under `docs/` contain detailed domain behaviour.

When a decision becomes permanent, document it in the appropriate documentation file rather than adding historical explanation to `CLAUDE.md`.

## 11. Working method

Work on one defined task at a time.

Before coding:

1. Identify the relevant files.
2. Explain the intended change briefly.
3. Check whether existing abstractions already solve part of the problem.
4. Implement the smallest coherent change.
5. Add/update tests.
6. Run the relevant verification.
7. Report exactly what was changed and what was verified.

Do not perform unrelated cleanup or refactoring while implementing a task.

Do not modify documentation merely to reflect transient implementation details.

## 12. Git

Commit messages:

- English.
- Imperative mood.
- Maximum 72 characters for the subject.
- One commit = one coherent idea.
- Do not mix unrelated formatting or cleanup with behavioural changes.

Do not create commits unless explicitly requested.

## 13. Current milestone

The authoritative current state is:

`docs/coding/current-state.md`

Do not infer the current milestone from the conversation history. Read the file.
