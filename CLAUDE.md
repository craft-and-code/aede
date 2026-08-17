# Working conventions — Aède

This file is for Claude Code and for any contributor. It states the rules of the project: what is settled, what must not be broken, and how Rust is written here.

Where this file contradicts a general habit, **this file wins**. Where a rule is clearly wrong for a specific case, say so explicitly rather than quietly working around it.

---

## 1. The project on one page

Aède is a local music library: give it folders, it builds a navigable catalog. The long-term ambition is to cover what Roon does, locally, with no subscription and on open metadata.

**Current state: milestone M0.** Folder scanning, native tag reading, graph model, statistics, diagnostics, command-line navigation.

**Deliberately out of scope for now:** any audio playback, any network access, any external database. Do not introduce them "while passing by" — each is a milestone of its own (see the roadmap in the README).

Rust 1.89 or later (`rust-version` in the workspace manifest, `msrv` in `clippy.toml` — the two must stay in step).

Domain vocabulary follows MusicBrainz: a `release` is what a user calls an album, a `recording` is a recorded performance, a `track` is the position of a recording within a release. Sticking to this avoids expensive misunderstandings at M1.

## 2. Commands

```sh
cargo build                      # offline once the dependencies are fetched
cargo test                       # 136 tests
cargo doc --no-deps --open       # the API documentation
cargo fmt --all                  # rustfmt.toml
cargo clippy --all-targets -- -D warnings
tools/check.sh                   # all four at once, before committing
tools/demo-library.sh /tmp/demo-music   # test library (needs ffmpeg)
```

`tools/check.sh` must pass before every commit. No exceptions.

## 3. Invariants

These properties are covered by tests. Breaking them breaks the project — if a change requires it, raise it first.

**A dependency is a requirement, not a dogma.** Adding one is a decision to argue for, not a reflex. Three criteria, all three of them: it does something we could not do as well ourselves; it is maintained and widely used; its own dependency tree is small enough to read. Propose it, say what it replaces, and ask before adding it.

Current list: `lofty`, for the containers we have no parser of our own for. Planned: `serde` at M2, when the HTTP contract makes hand-written serialization the wrong trade.

**The hand-written parsers stay.** `lofty` is a fallback reached only when the signature matches none of them, and it must never become the primary path for FLAC, MP3, MP4, Ogg, WAV or AIFF. Those parsers extract things no general-purpose library exposes — the LAME encoder delay and padding, the ALAC magic cookie, the Opus pre-skip — and M3 needs them for gapless playback. A format one of them claims and then fails on keeps its own diagnosis: the fallback is for `UnrecognizedFormat`, nothing else.

**The model is a graph.** The `credit` table (who does what, on what) and the `relation` table (typed links between entities) are the heart of the system. Never "simplify" towards an album → artist hierarchy: that graph is precisely what will let a user click a drummer and see their forty appearances.

**Construction is deterministic.** Two scans of the same library produce exactly the same identifiers. Consequences: sort before iterating, never let `HashMap` iteration order leak into output or into identifier assignment, and prefer `BTreeMap`/`BTreeSet` wherever order matters.

**Parsers never panic.** No `unwrap`, `expect`, `panic!`, direct indexing or unchecked slicing anywhere in `tags/` or `audit/`. A truncated or corrupt file yields an error or a partial result. Use the `Cursor` from `tags/bytes.rs`, whose reads all return `Option`. Use `checked_`/`saturating_` arithmetic on any value that comes from a file.

**The on-disk format is versioned.** Any incompatible change to the catalog bumps `store::FORMAT_VERSION`, and reading an older file must produce a clear message rather than a crash.

**The persisted JSON mirrors `schema.sql`.** One key in the file equals one table in the schema. Adding a field to the model means reflecting it in both. That is what will make the move to SQLite mechanical.

**Roles are not interchangeable.** `model::is_performing_role` separates being audible on a recording from having written or produced it. The distinction drives the artist page, the performer rankings and the collaboration graph; ignoring it puts other people's albums in an artist's discography.

**Scanning never silently narrows the library.** The watched folders live in `Catalog::roots` and accumulate. Dropping one is always an explicit act.

**`Various Artists` is not an artist**, it is the absence of an album artist. The same reasoning applies to any placeholder name: do not pollute the catalog with entities that are not entities.

## 4. Writing Rust here

### Language

Everything in this repository is in **English**: identifiers, comments, documentation, error messages, program output, command names, tests, commit messages. The project is published on GitHub and has to be readable by anyone.

Comments explain the **why**; the what is already in the code. A comment must be about the code, never about the conversation that produced it. Do not document tooling choices, alternatives that were rejected in chat, or anything a reader of the repository has no context for.

Good:

```rust
/// Two tracks count as duplicates when the same artist and title come back
/// with a close duration (less than 3 seconds apart).
///
/// The duration is what makes this safe: without it, a live rendition would
/// be flagged as a duplicate of the studio version.
```

No commented-out code left behind. No `TODO` without a sentence saying what is missing and why it is not done yet.

### Documentation

`//!` at the top of every module, saying what it is for and why it is built that way. `///` on every public item — `aede-core` sets `#![warn(missing_docs)]`, so an undocumented public item is a warning and `tools/check.sh` fails on it. Doc examples are compiled by `cargo test`, so they must stay correct.

A doc comment must add what the name does not give. `/// The artist's id.` on `artist_id: Id` is noise; say what it points at and why the field exists. Browse the result with `cargo doc --open`.

### Errors

One error type per module: an explicit enum implementing `Display` and `std::error::Error`, with `From` for the common conversions. Messages address the user in plain language and say **what to do**:

```rust
Err(format!(
    "no catalog in {}.\nRun this first: aede scan <folder>",
    dir.display()
))
```

`unwrap` and `expect` are forbidden in library code, tolerated in tests (with a message saying what was expected) and in `main` when the failure is fatal and already explained.

Never swallow an error silently. When work continues in spite of one — an unreadable folder must not abort a whole scan — report it back to the caller.

### API and style

Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/):

- naming: `snake_case` for functions, `CamelCase` for types, `SCREAMING_CASE` for constants; no `get_` prefix; `as_` (borrow, free), `to_` (expensive), `into_` (consumes);
- take `&str` over `&String`, `&[T]` over `&Vec<T>`;
- return iterators or `Vec` depending on what is easiest to use, not on what is most elegant to write;
- minimal visibility: `pub(crate)` by default, `pub` only for what genuinely belongs to the interface;
- derive `Debug`, `Clone`, `PartialEq` liberally on data types;
- borrow rather than clone, but do not contort the code to avoid cloning a small `String` on a cold path — readability first, targeted optimisation second.

`unsafe` is forbidden, with one documented exception already in the tree: restoring the default `SIGPIPE` handler in `aede-cli/src/main.rs`. Any other use must be justified in a `// Safe:` comment stating the invariant being upheld, and discussed first.

### Concurrency

`std::thread::scope` and the `std::sync` primitives are enough. No async runtime until networking arrives at M1, and then only after discussion. A poisoned `Mutex` must not bring down a scan: use `unwrap_or_else(|e| e.into_inner())`.

## 5. Tests

- unit tests live in the module, under `#[cfg(test)] mod tests`;
- integration tests live in `tests/`, and run against **real files** — the audio fixtures were produced with ffmpeg and cross-checked with ffprobe;
- a new format means a new fixture: reading it correctly is not something a unit test can claim;
- test names describe the behaviour: `truncated_vorbis_comments`, `various_artists_is_not_an_artist`;
- any non-obvious assertion carries a message with the observed value: `assert!(ok, "bitrate obtained: {bitrate} kbps")`;
- **every bug fix starts with a failing test.**

For a binary parser, always think through the three hostile cases: truncated input, a lying length field, a forged signature.

## 6. Command-line interface

Output is for humans first: aligned tables (`ui::Table`), colours that vanish outside a terminal or under `NO_COLOR`, correct plurals (`ui::plural`). Alignment is computed in **display columns**, not bytes — "Björk" takes five columns for six bytes.

`--json` on the query commands, for machine use. A misspelled option is reported, never silently ignored. The program does not panic when its output is cut off (`aede stats | head`).

## 7. Git

Commit messages in English, imperative mood, subject line of 72 characters at most, then a blank line and a body explaining **why**:

```
Treat "Various Artists" as the absence of an album artist

Recording it as an artist made it show up in the rankings and inflated the
artist count of every compilation.
```

One commit, one idea. Automatic reformatting and lint fixes go in their own commit, never mixed with a behaviour change.

## 8. Things to watch for in the coming milestones

**M1 — MusicBrainz.** A hard limit of one request per second and a mandatory identifying `User-Agent`, on pain of being blocked. Wikipedia is CC BY-SA: attribution is mandatory and carries over to translations. Go through Wikidata to reach the article in the user's language — it already exists in the vast majority of cases, written by humans, and no machine translation is then needed.

Matching a file to a release is the hard problem of this project. Always keep a confidence score and a way to review: never overwrite correct tags on the strength of an approximate match.

**M2 — API.** The HTTP contract freezes early and is versioned. Every future client will depend on it.

**M3/M4 — playback.** The encoder delay and padding (already extracted from LAME tags and the Opus pre-skip) are what make gapless playback possible. Do not lose them along the way.
