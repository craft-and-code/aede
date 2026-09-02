# Architecture

```
crates/
  aede-core/        library, no terminal I/O
    src/tags/         one parser per format -> RawTags
      foreign.rs        the formats handed to lofty
    src/audit/        what a file contains, as opposed to what it claims
      integrity.rs      the checksums the containers carry
    src/model/        the graph: entities, credits, relations
      mod.rs            the entities, and the catalog that holds them
      query.rs          reading the graph (&self throughout)
      builder.rs        scanned files -> entities, deterministically
      relations.rs      the links inferred from credits and track lists
    src/scan.rs       directory walk, parallel reads, incremental
    src/store.rs      JSON persistence, atomic writes
    src/stats.rs      statistics
    src/doctor.rs     diagnostics
    src/analysis.rs   analyses imported from another tool
    src/sources.rs    what other sources say, beside the tags
    src/musicbrainz.rs  reading its answers and matching them, no network
    src/http.rs       the one place a request is made (feature `fetch`)
    src/text.rs       name normalization (how entities are matched)
    src/json.rs       minimal JSON reader and writer
    src/clock.rs      the one unit of time the catalog stores
    schema.sql        a relational mirror of the model, kept as documentation
  aede-cli/         the `aede` binary
    src/commands/     one module per group of commands
tools/              development scripts
CLAUDE.md           project conventions and invariants
```

## The decision that shapes everything

The model is not "an album belongs to an artist" but **entities carrying roles towards one another**. Two tables do all the work:

- `credit` — _who does what, on what_: performer, composer, conductor, producer, engineer…
- `relation` — _typed links between entities_, with a weight and a source.

This is what will let you click a drummer and see their forty appearances. A flat schema would have to be thrown away the day MusicBrainz arrives.

The graph is already usable at M0, with no network access at all: `aede artist "Queen"` shows "played with David Bowie", inferred purely from the fact that both are credited on the same track. `aede artist "Queen" --with "David Bowie"` then lists those tracks. The relation itself stores only a count; the tracks behind it are recomputed from the `credit` table on demand, so a corrected tag never leaves a stale list behind.

## A few choices worth knowing about

**The catalog is a JSON file whose every key is a table.** Not a stopgap: it mirrors `schema.sql` exactly, so if a database is ever wanted it means rewriting `store.rs` and nothing else. Whether it ever will be is answered further down, by measurement rather than by taste.

**Raw tags are kept per file.** The graph can therefore be rebuilt entirely without touching the disk, which is what makes the incremental scan possible and will allow undoing an automatic correction.

It is also what makes an upgrade painless. The `relation` table is **inferred** — collaborations from the credits, album copies from the track lists — so a new version of Aède infers more than the stored catalog holds. Rather than refuse to load it, or wait for a rescan the user has no reason to suspect is needed, the catalog carries the version of the rules it was built under: when they no longer match, the relations are recomputed on load, from what is already in memory. Nothing is re-read, and the integrity verdicts — which a rebuilt catalog would lose — stay where they are.

**Scanning is incremental by default.** Same path, size and modification time means the file is not read again. On the demo library: 63 ms on the first pass, 2 ms on the second. `--full` forces a re-read.

**Watched folders accumulate.** `aede scan ~/Music` then `aede scan ~/Live` watches both; a plain `aede scan` refreshes everything. `--replace` keeps only the folders given, and `aede roots --remove` drops one. A watched folder that has become unreachable aborts the scan rather than quietly emptying the catalog.

Dropping a folder from the list does not empty the catalog on its own: its files stay until the next `aede scan` rebuilds the catalog from the folders still watched. Run that scan **without naming a folder** — naming the one just dropped would simply watch it again.

**A row measures what it counts.** In _Appears on_, the duration and the size cover the tracks the artist is on, not the whole album: one guest song is one guest song, not forty minutes of somebody else's record.

**The same album twice is two albums, and the model says why.** A library holds the same record more than once for two opposite reasons: a hi-res copy kept beside the CD rip, or a folder copied and forgotten. Aède keeps both as separate releases — they are two sets of files, in two folders, and the folder is what you act on — and links them with a typed relation:

| Same title, same album artist, same track list | Link            | What it means                                      |
| ---------------------------------------------- | --------------- | -------------------------------------------------- |
| Same encoding on both sides                    | `duplicate`     | Nothing tells the copies apart: one is dead weight |
| Different encoding                             | `other_edition` | The second copy is there on purpose                |

Track positions and titles must match exactly; durations only have to be within three seconds, since two rips of one disc never agree to the millisecond. Two albums merely sharing a name are left unlinked — without MusicBrainz there is nothing reliable to say about them.

`doctor` reports a `duplicate` once, as a warning naming both folders and the space to be recovered, instead of once per track — a copied album used to produce thirteen identical lines. An `other_edition` is reported too, as information: it is a choice, not a defect. The artist page and the album listing mark the rows, and `aede album` names the other folders.

**A guest appearance is not part of a discography.** Singing one track on somebody else's album puts it under _Appears on_, never under _Discography_, and writing or production credits go in a third section. The performer rankings count performing credits only.

_Performing_ means credited in a role that makes the artist audible on the recording: `artist`, `albumartist`, `performer`, `featured`, `conductor`, `remixer`. A composer or a lyricist is not audible, so an artist page states its figures on two labelled lines — `performing:` and `writing:` — rather than one unlabelled count. A band's lyricist genuinely has no performing credit: what the files say is that the band played, not who in it.

**A name is matched exactly first.** `aede album "Danzig"` shows the 1988 record, not whichever of `Danzig 4` or `Danzig II` the catalog holds first — an exact title ends the search. Only when nothing matches exactly does it widen to the titles containing the text, and then it shows **all** of them, saying that it widened. `aede track` follows the same rule.

**A title is not an identifier.** `aede track "So What"` prints every track carrying that name — the studio take, the single, the live rendition — because they are different recordings. `--artist` and `--album` narrow it down, and a list cut short by `--limit` always says so.

**Construction is deterministic.** Files are sorted before processing, so two scans of the same library produce exactly the same identifiers. Without that, no readable diff and no reproducible test.

**`Various Artists` is not an artist**, it is the absence of an album artist. Recording it would pollute every count.

**Sizes are decimal, durations are rounded.** 1 kB is 1000 bytes, the convention macOS Finder and most Linux file managers use, so a figure here matches what the system says about the same files. A duration is rounded to the nearest second rather than truncated, as players do.

**Every listing carries the same measures.** Count, duration and size on disk appear on each entity page and in each listing: what a slice of the library weighs should not depend on the command used to look at it.

**An artist is counted once per track.** Miles Davis as both performer and composer of "So What" is one track, not two.

## Tooling

```sh
tools/check.sh        # formatting, lint, tests, documentation, release build
cargo doc --no-deps --open   # the API documentation
```

Every public item of `aede-core` is documented: the crate sets `#![warn(missing_docs)]`, so a gap is a warning and `tools/check.sh` fails on it. The check also builds the documentation with `RUSTDOCFLAGS="-D warnings"`, which makes a **broken link** an error too — nothing else reads doc comments, so moving an item between modules would otherwise leave dead references behind in silence.

Formatting is `rustfmt` (`rustfmt.toml`); Prettier only covers Markdown, JSON, YAML, HTML and CSS (`.prettierrc`). The project targets **zero clippy warnings**.

## When this becomes a database

The catalog is one JSON file, loaded whole into memory by every command and rewritten whole by every scan. The roadmap put "move to SQLite" inside M1. Measurement moved it back out, and the figures are worth keeping because they say something other than what was expected.

A synthetic library — twelve tracks an album, ten albums an artist, the thirteen tags a well-tagged file carries — built through the real `builder` and saved through the real `store`. Each figure comes from a **fresh process**, because a first attempt that built and loaded in one run held two catalogs at once and overstated the memory by a third:

| tracks  | catalog.json | scan: save | peak while scanning | load    | peak while loading |
| ------- | ------------ | ---------- | ------------------- | ------- | ------------------ |
| 10 000  | 12.4 MB      | 0.79 s     | 160 MB              | 0.41 s  | 181 MB             |
| 50 000  | 62.5 MB      | 3.88 s     | 787 MB              | 2.17 s  | 897 MB             |
| 200 000 | 252.0 MB     | 16.37 s    | 3 127 MB            | 13.42 s | 3 586 MB           |

About 1.25 kB on disk per track — linear, no surprise. Two things in there are surprises.

**The disk is not the cost.** Reading those 252 MB takes 0.25 s; turning them into a catalog takes 13.2 s. That is this repository's hand-written parser at some 19 MB/s, where a serious one does thirty to fifty times better.

**The memory is the real ceiling.** Roughly 18 kB of resident memory per track, about fourteen times the file it came from — the whole graph, plus the complete JSON tree the parser materialises before converting it. At 200 000 tracks, `aede stats` wants three and a half gigabytes and thirteen seconds before it can print a single line.

So: **can this project do without SQLite?** For as long as "load the whole graph into memory" is an acceptable design — and the measurements say that holds comfortably to about 50 000 tracks, becomes uncomfortable somewhere past 100 000, and is untenable at 200 000. Which is an _architecture_ question, not a storage one, and it is worth being precise about what changes it:

- A **faster parser** and a parse that does not build the whole JSON tree first would take most of the thirteen seconds and a good part of the memory. It changes nothing about the ceiling itself.
- **SQLite** is the only thing that removes the ceiling, because it is the only one that stops requiring the whole graph to be resident.

The cheap lever comes first, and the roadmap already earmarks the moment: M2 brings `serde` in for the HTTP contract, and the `json` module was written to make that move mechanical.

There is also a second trigger that has nothing to do with size. `store::save` writes to a temporary file and renames, so a reader never sees a torn catalog — but two **writers** still clobber each other, and M2 puts a long-running server beside a CLI the user keeps using. That is a design decision to take at M2 (most simply: while the server runs, it owns the catalog and the CLI talks to it), and it does not by itself require a database either.

Two facts settle the shape of it when the time comes. `rusqlite` is not Rust: it compiles SQLite's C amalgamation with `cc`, and on musl the `bundled` feature is not optional — without it the crate links against a host SQLite and the static binary segfaults on startup. And there is no pure-Rust replacement worth the risk: the SQLite rewrite (Turso, formerly `limbo`) is at 0.7, labelled BETA by its own README, still missing `WITH RECURSIVE`, most window functions and custom collations — and its maintainers advise caution for anything mission-critical. The C-compiler objection, which is what made this look expensive a milestone ago, has meanwhile evaporated on its own: `rustls` already brings one.

## Dependencies

Two: `lofty`, for the tag formats whose parsers are not worth writing twice, and `ureq`, behind a feature. Everything else — the binary parsers, the JSON store, the query grammar, the table layout — is written here, and `tools/check.sh` builds with `--offline` so that a step which suddenly needs the network means a dependency was added without being discussed.

Where a program can do the job instead of a crate, the program wins: **ffmpeg is driven as an external process** (`core/ffmpeg.rs`, `find()` and `missing(what)`), never linked. Two commands use it, both say so when it is absent, and the other twenty-three do not care.

**M1 adds `ureq` with `rustls`**, and that is a deliberate departure worth writing down. The alternative considered was spawning `curl` the way ffmpeg is spawned — zero crates, same precedent — and it was refused on one asymmetry: ffmpeg is optional and covers two commands, whereas identification _is_ M1. Making the milestone's whole purpose depend on a program that may not be on the machine is a different promise from making `--compress` depend on one. What that costs, stated plainly: about twenty-seven transitive crates, and the project's first requirement of a **C compiler** at build time — `rustls`' crypto providers all carry C or assembly, and no pure-Rust provider is production-recommended today. The release workflow already installs `musl-tools`, so the cost on CI is nil.

Two precautions come with it. `ureq` reserves the right to change its crypto provider in a minor release, so the version is pinned and the provider named explicitly rather than inherited from `default`. And the "one dependency" line disappeared from the README and the site the day it landed — a claim that is no longer true is worse than no claim.

## What GitHub does

Three workflows, and each does one thing.

`ci.yml` runs `tools/check.sh` on every push and pull request, on Linux, macOS and Windows. It calls the script rather than restating its steps: a CI that listed formatting, clippy, the tests and the build a second time would drift from the script contributors actually run, and then "green here, red there" becomes a normal state. One gate, not two. Windows has no bash script and runs `cargo test` instead, which is the point of that leg anyway — the parsers and the path handling on a system where a separator is a backslash. The toolchain is pinned to **1.89**, the MSRV the manifest declares: a CI floating on stable would let a newer feature slip in and break the promise without anybody noticing.

`release.yml` fires on a tag matching `v*` and builds three archives — macOS on Apple Silicon and Intel, and Linux — each holding the binary, the licence and the manual's front page, with a `.sha256` beside it. Windows is deliberately absent, and [Paths](paths.md) says why: it compiles, CI still runs its tests, but catalog paths are `/`-separated by design while the scanner stores the platform's own spelling, so everything folder-shaped is wrong there. Three decisions are worth keeping:

- **Linux is built against musl, not glibc.** A `gnu` build made on Ubuntu 22.04 refuses to start on anything older — a Debian 11 server, a NAS — with a message about `GLIBC_2.34` that means nothing to whoever downloaded it. Everything here is pure Rust, so a static build costs nothing.
- **The tag and `Cargo.toml` must agree**, checked before anything is compiled. Otherwise `v0.2.0` publishes a program that answers `0.1.0` to `--version`, and `--version` is what a bug report quotes.
- **The tests run before the archive is made.** A tag that does not build is worse than no tag: it is published, people download it, and the failure is theirs to discover.

The release is published as a **draft**, with the commit list generated under a `<!-- TODO -->` placeholder. A body that wrote itself entirely would be a commit list, and a commit list is not release notes; the draft forces one deliberate pass over "what changed" before anyone sees it.

`site.yml` publishes `site/` to the root of `gh-pages` whenever that folder changes. The page is one HTML file, one stylesheet and a few images, with the system font stack and no third-party request of any kind — nothing to build, and nothing for a visitor's browser to fetch from anywhere but the project's own domain. `keep_files: true`, so that anything else ever published under that branch survives a site deploy.

## Tests

```sh
cargo test
```

467 tests: binary parsers (including truncated files and forged signatures), name normalization, graph construction, persistence round-trip, statistics, diagnostics, table alignment, argument parsing, an end-to-end test that runs the binary, and a check that no link in this manual leads nowhere. The conversion tests skip themselves, loudly, when ffmpeg is not installed.
