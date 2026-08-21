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
cargo test                       # 217 tests
cargo doc --no-deps --open       # the API documentation
cargo fmt --all                  # rustfmt.toml
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps   # broken links are errors
tools/check.sh                   # all five at once, before committing
tools/demo-library.sh /tmp/demo-music   # test library (needs ffmpeg)
```

`tools/check.sh` must pass before every commit. No exceptions.

It includes `cargo doc` because a broken documentation link is silent everywhere else — neither the build nor clippy reads them — and moving an item between modules is precisely what breaks one. In a codebase where the reasoning lives in the doc comments, a dead link is a real defect.

## 3. Invariants

These properties are covered by tests. Breaking them breaks the project — if a change requires it, raise it first.

**A dependency is a requirement, not a dogma.** Adding one is a decision to argue for, not a reflex. Three criteria, all three of them: it does something we could not do as well ourselves; it is maintained and widely used; its own dependency tree is small enough to read. Propose it, say what it replaces, and ask before adding it.

Current list: `lofty`, for the containers we have no parser of our own for. Planned: `serde` at M2, when the HTTP contract makes hand-written serialization the wrong trade.

**The hand-written parsers stay.** `lofty` is a fallback reached only when the signature matches none of them, and it must never become the primary path for FLAC, MP3, MP4, Ogg, WAV or AIFF. Those parsers extract things no general-purpose library exposes — the LAME encoder delay and padding, the ALAC magic cookie, the Opus pre-skip — and M3 needs them for gapless playback. A format one of them claims and then fails on keeps its own diagnosis: the fallback is for `UnrecognizedFormat`, nothing else.

**The model is a graph.** The `credit` table (who does what, on what) and the `relation` table (typed links between entities) are the heart of the system. Never "simplify" towards an album → artist hierarchy: that graph is precisely what will let a user click a drummer and see their forty appearances.

`model/` is divided by verb, and the division is load-bearing rather than cosmetic: `query.rs` takes `&self` throughout, so a lookup that tried to change something would not compile; `builder.rs` is the only place identifiers are handed out; `relations.rs` holds what is inferred rather than read, which is why it is the thing that carries `RELATION_RULES`. A new function goes to the file whose verb it is, not to whichever one is shortest. What the rest of the program calls is re-exported from `model/mod.rs`, so callers never name the sub-modules.

**A test asserts on what survives the display.** Path columns truncate from the **left**, so the last components — the ones that identify the thing — are what remains on a narrow terminal. A test matching a whole path therefore passes on Linux and fails on macOS, where a temporary path alone is sixty columns; this has now cost two rounds. Match the tail, or assert on something the renderer never shortens.

**A folder is not a prefix of its name.** `text::is_under` is the only way to ask whether a path sits in a folder. The obvious `path.starts_with(root)` is wrong on strings — `/music/Rock` claims every file of `/music/Rockabilly` — and `aede roots` counted a neighbour's files that way while `check` had already got it right inline, which is how one idea becomes two. The test is on a separator boundary, in one place.

**One idea, one implementation.** `text::file_name` and `text::folder` split a path; `clock::now_seconds` says what time it is. Both had grown three copies in three files, each a little different — which is how "the same thing" quietly becomes two things. A helper short enough to retype is exactly the one that gets retyped: put it where it belongs and use it.

**Construction is deterministic.** Two scans of the same library produce exactly the same identifiers. Consequences: sort before iterating, never let `HashMap` iteration order leak into output or into identifier assignment, and prefer `BTreeMap`/`BTreeSet` wherever order matters.

**Parsers never panic.** No `unwrap`, `expect`, `panic!`, direct indexing or unchecked slicing anywhere in `tags/` or `audit/`. A truncated or corrupt file yields an error or a partial result. Use the `Cursor` from `tags/bytes.rs`, whose reads all return `Option`. Use `checked_`/`saturating_` arithmetic on any value that comes from a file.

**Inferred data carries the version of the rules that inferred it.** `model::RELATION_RULES` is bumped when the way relations are derived changes; `store::from_json` then recomputes them, in memory, from the credits and tracks it just read. This is deliberately not `FORMAT_VERSION`: an out-of-date inference is stale, not invalid, and refusing to load would cost the user their integrity verdicts. The rule generalises — anything derived rather than read belongs here, not in the format version.

**The on-disk format is versioned.** Any incompatible change to the catalog bumps `store::FORMAT_VERSION`, and reading an older file must produce a clear message rather than a crash.

**A destructive command says what is lost, then asks.** `reset` lists what the catalog holds before removing it, and distinguishes what a rescan brings back from what it does not — the watched folders, the integrity verdicts and the imported analyses. With no terminal to ask on it refuses instead of assuming: assuming "no" makes a scripted reset fail without saying why, assuming "yes" removes something nobody agreed to lose. `--yes` is the explicit consent.

**A short option is the long one written shorter.** `args::SHORT` resolves it to a long name and everything downstream — values, guards, the missing-value check — sees one option. Aliases stay few: four saved keystrokes cost a documented line for ever, so they are worth it only where the option is typed constantly (`-o`, `-j`, `-h`, `-V`).

**The help is part of the contract.** An option printed under a heading that names the one command it does not work on is a lie that costs more than a missing line. Each option in `print_help` says where it applies, and a test asserts the help names every filter option the parser accepts — the same claim, checked from both ends.

**An option a command cannot honour is refused.** `main.rs` holds `CSV_COMMANDS`, `M3U_COMMANDS` and `OUTPUT_COMMANDS`: the global option list only says an option exists, these say where it means something. Accepting `--csv` on `stats` and doing nothing is the same fault as swallowing a misspelled option — worse, in fact, since the command then reports success. Adding an option means adding it to a list here, or it will be silently ignored somewhere.

**Every entity deserves a page, and every page a filter.** The model is a graph; a listing that counts genres without letting you open one is a dead end. `artist`, `album`, `track`, `genre` and `label` each have a singular page, and what a page gathers is a selection — so `--csv` and `--m3u` work on it, through `commands::selection_output`, without the command knowing anything about them. Adding an entity kind means adding its page.

**What is shown is what is accepted.** `commands::ROLE_NAMES` is one table read in both directions — `role_label` for display, `role_key` for input — because a one-way `match` produced a message that denied a role and listed it in the same breath: the screen said "album artist", the parser wanted "album". Anything the interface prints as a name must be typeable back in, and an error offering alternatives offers them in the spelling it displays them in. The same applies to any vocabulary added later.

**A role is a question asked in both directions.** `Catalog::artists_in_role` answers "who does this here", `Catalog::tracks_of_artist_in_role` answers "what did this person do in that role", and `--role` carries both readings depending on whether it is attached to the listing or to a page. That inversion is the whole reason the `credit` table stores a role rather than being a bare artist column. `Catalog::roles_in_use` reads the vocabulary from the credits rather than from a fixed list — a role arriving from MusicBrainz at M1 must work without a line of code. A role needs a person, which is why `album` and `track` refuse it and say so: there, `--artist` is the filter.

**A truncated list says so, and says where it stopped.** `args::Window` is the one reading of `--limit`, `--offset` and `--all` for the whole program, and `commands::announce_window` the one way of reporting it: `1–50 of 312 albums`, nothing at all when everything fit, and an explicit line when the window falls past the end. All five listings used to stop in silence, and sorted by year that made the most recent albums of a real library invisible — the header counting the matches is not enough, since nobody compares it against the rows they were handed.

Paging is only meaningful because construction is deterministic: page two is the rows after page one, and that holds only while every listing sorts. The window is read **strictly** — `--limit abc` is an error, not a fall-back on the default — and `--limit=0` is refused in favour of `--all`, because a size of zero is never what anyone means and "everything" deserves a name rather than an encoding. Any new listing goes through `Window` and that helper.

**A value that is a name may be typed in several words.** `args::VALUED_NAME` (`--artist`, `--album`, `--with`, `--genre`, `--label`) takes every word up to the next option; `args::VALUED_WORD` takes exactly one, because a number, a path or a keyword is one word. The bug this fixes was the worst kind: `artist Ozzy --with Jeff Beck` gave `--with` the word "Jeff", left "Beck" to be joined onto the positional, and went looking for an "Ozzy Beck" nobody had typed. A wrong answer built in silence is worse than a refusal — and here it is worse than being permissive too, since the shell has already split the words and only this program knows they were one name. Adding a name-valued option means adding it to the right list.

**An option means the same thing wherever it is typed.** `--m3u` and `--csv` on `album`, `artist`, `track` and `search` all describe the tracks on screen, through one helper (`commands::selection_output`); `export` describes the catalog. A command that cannot honour an argument says so — `aede export "an album"` is an error, never a full export under a name that promised a selection.

**Each export answers one question.** JSON is the faithful dump and mirrors the model; CSV is a flat table for a spreadsheet, denormalized on purpose and carrying raw values, since a formatted column cannot be summed; M3U exports a selection, not the catalog. Adding a format means saying which question it answers that the other three do not.

**An argument a command cannot read is refused.** The twin of the rule above, and it bit later: `main.rs::takes_no_argument` names the commands that read only their options, and a positional given to one of them is an error pointing at the command that does take it. `aede artists ozzy --role producer` listed every producer with "ozzy" going into the void — an answer that looks right is worse than one that fails.

**A count of zero is an answer; an empty screen is not.** `stats` prints the credit vocabulary the library actually holds, so `--role composer` returning nothing can be told apart from a bug. Wherever a filter can legitimately match nothing, something must let the user see the domain it filters over — otherwise every empty result reads as a defect, and the user is right to think so.

**A filter is visible or it is not a filter.** `albums` prints the facets it narrowed on, because a filter that leaves the count unchanged cannot be told from one that was ignored — and this project shipped `--genre` and `--label` declared in the option list and honoured nowhere for months. A filter matching nothing is an **error** naming where to look, never an empty listing: an empty listing reads as an empty library.

**A folder given to a command is walked whole.** `import` recurses, because reports are filed the way albums are — one folder per artist, one per album — and a walk that stopped at the top level would report "nothing found" on the folder the user actually meant. The same applies to any folder argument added later.

**The persisted JSON mirrors `schema.sql`.** One key in the file equals one table in the schema. Adding a field to the model means reflecting it in both. That is what will make the move to SQLite mechanical.

**The same album twice is two releases and one relation.** Never merge two folders into one release: they are two sets of files, and the folder is what the user acts on. `model::DUPLICATE` and `model::OTHER_EDITION` say which case it is — same track list and same encoding, or same track list and a different one. Reporting belongs to the album level: a copied album is one issue, not one per track, and an other edition is information rather than a warning.

**Roles are not interchangeable.** `model::is_performing_role` separates being audible on a recording from having written or produced it. The distinction drives the artist page, the performer rankings and the collaboration graph; ignoring it puts other people's albums in an artist's discography.

A figure derived from it carries the name of the role class it counts. An artist page shows `performing:` and `writing:`, never a bare total: tags credit the band, not its members, so a lyricist with forty albums has zero performing credits and the unlabelled zero read as an error.

The two lines are **not a partition**, and must not be made into one. Someone who writes what they sing belongs in both, because writing it and playing it are two facts about the same track, not two halves of one. The page also holds *display* sets that do subtract — `releases_written_without_performing`, `written_tracks_without_performing` — so a table does not repeat what the table above it already showed. Those exist to be printed, never to be counted: reporting one as `writing:` announced Ozzy Osbourne, sixty-nine composer credits, as writing a single track. A method whose result is a display set says so in its name, and a summary line reads from the measure, never from the size of a table.

**"Not checked" is not "nothing to check".** An absent integrity verdict means no one has looked yet, and that can change; `Verdict::NothingToCheck` means the container carries no checksum, and that never will. Collapsing the two would have the user re-running `aede check` forever on their MP3s. The same distinction holds in the JSON, in `schema.sql` and in whatever the interface ends up drawing.

**A verdict belongs to the bytes it was reached on.** `integrity` travels with the file entry: a scan that reuses an unchanged file keeps it, a scan that re-reads a modified one drops it. Never carry a verdict across a change of size or date. An imported analysis follows the same rule from the other side: `FileAnalysis::still_applies` compares the size and date the other tool saw against the ones the catalog holds, and nothing acts on a record that fails it.

**A measurement carries who made it.** Imported analyses live in their own table, attributed to their source, and are never merged into Aède's own fields. A bit depth read from the wasted-bits counts and one obtained by decoding are two different claims; merging them loses the provenance and, worse, hides the case where they disagree. Noticing the disagreement is the whole reason to keep the data. `doctor` reports an MD5 mismatch as an **error** even when `aede check` said intact — the frame checksums prove the container, the MD5 proves the audio, and passing one while failing the other is a finding, not a contradiction to arbitrate.

**A scan may not destroy what it cannot recompute.** Imported analyses travel with the catalog a scan rebuilds. Tags, durations and the graph all come back from reading the files again; an hour of someone else's decoding does not. Anything future that is entered rather than read belongs in that same carry-over.

**Entered data is keyed by path, not by identifier.** Identifiers are positions that every scan renumbers, so anything not rebuilt by the scan would have to be remapped after it — and, worse, could not exist before it. `FileAnalysis` therefore carries the path it describes and no id. That is what lets a report be imported into an empty catalog and attach itself later, which makes the order of "analyse" and "scan" irrelevant. A record whose file the catalog does not hold is waiting, not broken: it is counted (`Catalog::pending_analyses`), reported, and never diagnosed as a defect.

**Matching a file is not the same as describing it.** `analysis::merge_into` is the single place that decides which file a record is about — by path, then by name and size — and both routes end in the same `still_applies` test against that file's size and date. The fallback bypassing it was a real bug: a name and a size can agree while the modification date says the tags were rewritten yesterday. `analysis::reconcile` re-tries the waiting records after a scan, which is what makes a report written against a symbolic link (or `/var` against a canonical `/private/var`) attach at all. Never re-implement this matching in a caller — the scan and `import` share it precisely so the two cannot drift.

**Scanning never silently narrows the library.** The watched folders live in `Catalog::roots` and accumulate. Dropping one is always an explicit act, and a scan with no folder left is the way a catalog is emptied — refusing it would strand the files with no way out.

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

An elapsed time goes through `ui::elapsed`, which switches to seconds then to minutes: `260604 ms` makes the reader do the division. A long-running command says what it is about to do before doing it, in volume rather than in a predicted time — how long a read takes depends on the disk, and a wrong estimate is worse than none — and saves its progress as it goes, so that interrupting it costs only the current batch.

A column holding a path is bounded with `Table::path_limit`, which drops the **start** of the text: the file name is what identifies the file, and on macOS a temporary path is sixty columns before the name even begins.

`--json` on the query commands, for machine use. A misspelled option is reported, never silently ignored, and an option that expects a value but was given none stops the command: answering as if it had never been typed would be a wrong answer, not an incomplete one. Any option taking a value belongs in `args::VALUED`, or its space-separated form lands in the positionals. The program does not panic when its output is cut off (`aede stats | head`).

Sizes are **decimal** (`text::format_size`, 1 kB = 1000 bytes), matching Finder and the other tools of this family; dividing by 1024 under a "MB" label understates an album by 5% and reads as a bug. Durations are **rounded** to the nearest second, never truncated — truncating loses a second on half the tracks of a library.

A row measures the set of tracks it counts, never a wider one: in the *Appears on* table the duration, the size and the formats describe the artist's tracks, not the release they sit on.

Count, duration and size on disk go together: a command that shows one of the three shows all three, and `commands::totals` is what computes the last two. In a table a duration is `text::format_duration` (`h:mm:ss`, right-aligned); in a sentence it is `ui::long_duration` (`1 d 22 h 41 min`).

A lookup by name matches exactly first, and only widens when nothing matched — `Catalog::find_releases` and `Catalog::find_tracks` share that rule and report which of the two happened. Returning the first of several partial matches is the fault this replaced: an arbitrary answer, given without saying so.

The shape of an answer does not depend on how many results it has: `aede track` prints the same page whether one track carries the title or four. Any list bounded by a limit says so on screen — a silent truncation reads as "that is all there is".

## 7. Git

Commit messages in English, imperative mood, subject line of 72 characters at most, then a blank line and a body explaining **why**:

```
Treat "Various Artists" as the absence of an album artist

Recording it as an artist made it show up in the rankings and inflated the
artist count of every compilation.
```

One commit, one idea. Automatic reformatting and lint fixes go in their own commit, never mixed with a behaviour change.

## 8. Things to watch for in the coming milestones

**M1 — artist identity.** Tags carry names, not identifiers, and the same person arrives spelled several ways: `Ozzy Osbourne` / `O. Osbourne`, `Glen Benton` / `Benton` as credited on the sleeve. Matching on a fragment of the name is out of the question — it would merge Angus and Neil Young. The answer is a local alias file, applied when the graph is built so a `scan` propagates it, plus `doctor` **suggesting** candidates (one name a suffix of another, sharing releases) without ever applying them. Where MusicBrainz gives an identifier, the identifier wins over the name.

**M1 — MusicBrainz.** A hard limit of one request per second and a mandatory identifying `User-Agent`, on pain of being blocked. Wikipedia is CC BY-SA: attribution is mandatory and carries over to translations. Go through Wikidata to reach the article in the user's language — it already exists in the vast majority of cases, written by humans, and no machine translation is then needed.

Matching a file to a release is the hard problem of this project. Always keep a confidence score and a way to review: never overwrite correct tags on the strength of an approximate match.

**M2 — API.** The HTTP contract freezes early and is versioned. Every future client will depend on it.

**M3 — the decoder.** It is what the FLAC MD5 check waits for: verifying that hash means decoding the audio. The frame walk in `audit/flac.rs` already reads every Rice residual and throws the numbers away — what is missing is the LPC restoration, the inter-channel decorrelation and MD5 itself. Adding the stronger verdict must not change the stored shape, only `integrity_method`.

The two checks are not competing methods and neither replaces the other: the frame CRCs are the cheap pass that reads the container, the MD5 is the deep pass that reads the audio. Both stay. "Aligning on FlacCompagnon" therefore does not mean copying its verdict but computing the same digest over the same bytes — the interleaved little-endian samples the FLAC specification defines, which is the only thing there is to agree on. Two conforming implementations cannot disagree, and if they do, one of them has a bug: that is exactly the check `doctor` already performs on imported reports, and it is why the disagreement is reported rather than resolved. The way to guarantee it is to stop having two implementations at all — extract `audit` into a crate both programs depend on.

**M3/M4 — playback.** The encoder delay and padding (already extracted from LAME tags and the Opus pre-skip) are what make gapless playback possible. Do not lose them along the way.
