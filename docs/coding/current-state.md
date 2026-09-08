# Aède — Current State

This file is the short-term memory of the project.

It describes the state of the repository at the beginning of a development session. It is intentionally concise.

Do not turn this file into a development diary.

---

## Project status

**Current milestone:** M1 — Identification

**Previous milestone:** M0.6 — Catalog and local library

**Status:** M1 in progress

Aède has completed the local catalog foundation and has started the external identification layer.

The current implementation includes:

- folder scanning;
- native tag reading;
- graph-based catalog;
- releases, recordings, tracks and artists;
- credits and relations;
- catalog statistics and diagnostics;
- favourites, ratings, notes and user tags;
- listening history;
- saved collections;
- copying selections;
- catalog export;
- integrity checks;
- imported analyses;
- backup and restore;
- audio fingerprinting;
- MusicBrainz fetching;
- MusicBrainz discography information;
- Wikipedia/Wikidata summaries;
- cover-art fetching;
- language selection for fetched prose;
- artist identity: spellings merged on a shared MusicBrainz identifier, and
  `aede merge` for the files that never met one, with `aede doctor` suggesting
  pairs and applying none;
- dated band membership, and the line-up of an album's year, derived on read;
- lyrics fetching from LRCLIB, behind `--lyrics` and never on by default.

Audio playback is intentionally not implemented yet.

---

## Current M1 direction

M1 introduces identification and external metadata without replacing local data.

The fundamental rule is:

> External information is a claim beside local data, not a replacement for it.

Important consequences:

- local tags remain authoritative for local file metadata;
- external values are attributed to their source;
- disagreements remain visible;
- external data can be removed without destroying local metadata;
- approximate matching must retain confidence information;
- no automatic retagging is performed.

See:

- `docs/design/roadmap.md`
- `docs/annotating.md`
- `docs/sources.md`
- `docs/library.md`

---

## Test status

**Last recorded test count:** 663

**Last verified:** 2026-09-08

The count above is informational.

Do not change it after every implementation task.

Update this number only when deliberately recording a new project checkpoint, normally:

- at the end of a milestone;
- after a significant test-suite change;
- or when explicitly asked to update the project state.

The authoritative command is:

```sh
cargo test
```

If the test count changes during development, that does not by itself require this file to change.

---

## Verification status

The repository verification command is:

```sh
tools/check.sh
```

It covers the project's required formatting, tests, documentation and lint checks.

Before a milestone is considered complete, run:

```sh
tools/check.sh
```

Rust verification is performed locally when Claude Code does not have access to the Rust toolchain.

---

## Architecture state

### Catalog

The catalog is persistent and graph-based.

Important concepts:

- Release
- Recording
- Track
- Artist
- Credit
- Relation

Identifiers are deterministic across scans of the same library.

### Storage

The catalog and related stores use versioned JSON representations.

Current stores include:

- `catalog.json`
- `user.json`
- `sources.json`

Backup/restore operates on these stores independently.

### External sources

External information is stored separately from local file metadata.

Current external sources include:

- MusicBrainz
- Wikidata
- Wikipedia
- AcoustID
- Cover Art Archive

Network access is explicit and does not occur during ordinary local catalog operations.

---

## Important constraints

- Never modify audio files or their tags.
- Never silently overwrite local metadata with external data.
- Never silently ignore a CLI argument or option.
- Never introduce network access into local-only commands.
- Never add a dependency without justification.
- Never weaken tests to make an implementation pass.
- Never introduce unrelated refactoring into a task.
- Preserve provenance for external information.
- Preserve deterministic catalog construction.

---

## Working rule for Claude

At the beginning of a new session:

1. Read this file.
2. Read the relevant milestone/task document.
3. Inspect the existing implementation.
4. Work only on the requested task.
5. Update this file only when the project state itself has materially changed.

Do not reconstruct project history from previous conversations.

The repository and documentation are the source of truth.
