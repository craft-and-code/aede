# Aède

A local music library, written in Rust.

An _aède_ (Greek ἀοιδός, _aoidos_) was the poet-singer of archaic Greece: he held the whole repertoire in memory and performed it. Keeping and playing, in one word — which is exactly what this program is for.

**M0.6 is done**: read folders, turn them into a catalog of linked entities, answer questions about it, keep what you think of it, and get it back out to a player. No audio playback yet — that is deliberate, and the [roadmap](docs/design/roadmap.md) says when it arrives. The network is reached by exactly one command, `aede fetch`, and never on its own.

**M1 — identification — has started**, with the layer that receives it: a value fetched from MusicBrainz will sit _beside_ the tag, attributed and removable, never on top of it. The reasoning is in [The attributed layer](docs/design/attribution.md), and nothing in this first step touches the network.

The project has a page of its own: **<https://craft-and-code.github.io/aede/>** — what it does, and the roadmap.

## Getting started

`aede` is one executable. Unpack it, put it somewhere on your `PATH`, run it — nothing is installed and nothing runs in the background.

| Your system                         | Download                                                            |
| ----------------------------------- | ------------------------------------------------------------------- |
| **macOS** (M1 and later)            | `aede-*-macOS-AppleSilicon.tar.gz`                                  |
| **macOS** (Intel)                   | `aede-*-macOS-Intel.tar.gz`                                         |
| **Linux** (any 64-bit distribution) | `aede-*-Linux-x86_64.tar.gz` — statically linked, no glibc to match |

The builds are on the [releases page](https://github.com/craft-and-code/aede/releases), each with a `.sha256` beside it so a download can be checked before it is trusted. On macOS they are not signed with an Apple Developer ID, so Gatekeeper refuses the first run; `xattr -dr com.apple.quarantine ./aede`, once, settles it.

**Windows is not published yet.** It compiles and most of it works, but catalog paths are `/`-separated by design and the scanner stores the platform's own spelling, which makes everything folder-shaped — album grouping, `--folder`, imports, playlists — wrong on Windows. The reasoning, the twelve tests that prove it and the shape of the fix are in [Paths](docs/design/paths.md). Shipping a binary that builds the catalog wrongly, on the platform where nobody would think to check, is worse than shipping none.

```sh
tar xzf aede-0.1.0-macOS-AppleSilicon.tar.gz
./aede scan ~/Music
./aede stats
```

Or build it yourself. Rust 1.89 or later; the build downloads two dependencies — `lofty` for the tag formats, `ureq` for MusicBrainz — and everything after the first build works offline:

```sh
cargo build --release
./target/release/aede scan ~/Music
```

Two commands want `ffmpeg` on the `PATH` — `aede copy --compress` and `aede spectrum` — and both say so if it is missing. Nothing else needs it.

## Commands

| Command                                                   | What it does                                                                                                                                                                   |
| --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `aede album "<title>"`                                    | Tracks, durations, formats, credits                                                                                                                                            |
| `aede artist "<name>"`                                    | Discography, collaborations, roles (`--with <other>` lists the tracks two artists share)                                                                                       |
| `aede artists` / `albums` / `genres` / `labels` / `years` | Listings (`artists --role producer`, `albums --compilations`)                                                                                                                  |
| `aede check [folder…]`                                    | Verify the checksums the files carry (`--full` re-verifies everything)                                                                                                         |
| `aede collection <name>`                                  | Save a query under a name, run it, or drop it                                                                                                                                  |
| `aede collections`                                        | The saved queries, and how much each holds now                                                                                                                                 |
| `aede copy <destination>`                                 | Copy a selection to a player, a card or a drive, keeping its folder tree                                                                                                       |
| `aede doctor`                                             | Missing tags, duplicates, incomplete albums, mixed formats                                                                                                                     |
| `aede export`                                             | Export the catalog as JSON, or as CSV with `--csv`                                                                                                                             |
| `aede favourites` / `notes` / `history`                   | What you wrote, and what you played                                                                                                                                            |
| `aede fetch [name…]`                                      | Ask MusicBrainz about your artists **and albums** and store what it says beside your tags — the albums being where a tag can actually be contradicted (`--dry-run` lists what would be asked, `--full` asks again, a name narrows it and reaches the records as well as the person). One request per second, as the service requires. `--summaries` is a second pass that follows each artist's Wikidata link to a Wikipedia article and keeps its opening paragraph, with the page and the licence it is under |
| `aede missing [name…]`                                    | Studio albums MusicBrainz credits to your artists that this catalog does not hold. Fetches nothing: the answer is derived from what `aede fetch --discography` stored, so an album stops being listed the day you add it                          |
| `aede file <path>`                                        | Inspect a single file, outside the catalog                                                                                                                                     |
| `aede genre <name>`                                       | What is in a genre: albums and the artists audible on them                                                                                                                     |
| `aede help`                                               | Every command and every option, which is the contract                                                                                                                          |
| `aede import <report…>`                                   | Take in a FlacCompagnon report (`--list` says what is held and what became of it, `--pending` lists the folders whose analyses match no file yet, `--forget` removes analyses) |
| `aede label <name>`                                       | A label's catalogue and its artists                                                                                                                                            |
| `aede love\|rate\|note\|tag <kind> <name>`                | What you think of it: a favourite, 1–5 stars, a note, free labels (`tag` takes a comma-separated list)                                                                         |
| `aede played <track>`                                     | Record a listen, until playback records its own (`--remove` undoes the last one)                                                                                               |
| `aede playlist [folder…]`                                 | Write an `.m3u` in every album folder, in album order and with relative paths                                                                                                  |
| `aede query <expression>`                                 | Every track an expression matches, as a selection                                                                                                                              |
| `aede reset`                                              | Remove the catalog, after confirmation (`--yes` skips it)                                                                                                                      |
| `aede roots`                                              | List the watched folders (`--remove <folder>` to drop one)                                                                                                                     |
| `aede scan [folder…]`                                     | Scan the watched folders; any folder given is added to them                                                                                                                    |
| `aede search <text>`                                      | Search across the whole catalog (`--comments` looks in the comment tag, `--notes` in your own notes, `--lyrics` in the words)                                                  |
| `aede sources`                                            | What other sources say about your library, beside your tags and never on top (`--template` writes a document with the keys and nothing filled in, `--import <file>` takes one back, `--export` writes out what is held, `--list` shows each record, `--forget` drops them, `--source` narrows to one) |
| `aede spectrum [folder…]`                                 | Draw a spectrogram of every track into a `spectrograms/` folder beside it, through ffmpeg                                                                                      |
| `aede stats`                                              | Tracks, albums, formats, quality, decades, completeness                                                                                                                        |
| `aede track "<title>"`                                    | Every track carrying this title: album, credits, technical details, tags (`--lyrics` adds the words)                                                                           |

`--json` produces machine-readable output wherever `--csv` does — the same rows, typed — plus `stats`, `doctor`, `search` and `track`, which have shapes of their own. `aede help` lists every option.

The catalog lives in `$AEDE_HOME`, or `~/.local/share/aede/catalog.json`.

## Documentation

The pages below are the manual; this file is the front door. Every command is
also documented by `aede help`, which is the contract — a command that works is
a command the help names.

**Using it**

| Page                                                 | What is in it                                                                        |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------ |
| [Building the library](docs/library.md)              | Scanning, watched folders, folders never read, box sets, compilations, starting over |
| [Commands, options and output](docs/commands.md)     | Where each option applies, exporting as JSON, CSV or M3U, paging                     |
| [Browsing](docs/browsing.md)                         | Listings, facets, one row per album rather than per track                            |
| [Asking questions](docs/querying.md)                 | The query grammar, searching text, comments, saved collections                       |
| [What you think of it](docs/annotating.md)           | Favourites, ratings, notes, tags, and what a backup must keep                        |
| [Are the files still intact?](docs/integrity.md)     | `aede check`, what a checksum proves and what it does not                            |
| [What another tool found](docs/imported-analyses.md) | Importing FlacCompagnon reports, and what Aède says about them                       |
| [What other sources say](docs/sources.md)            | MusicBrainz, correcting it by hand, and what never lands on your tags                |
| [Copying to a player](docs/copying.md)               | `aede copy`, companion files, safe names, encoding on the way out                    |
| [Spectrograms](docs/spectrograms.md)                 | `aede spectrum`                                                                      |
| [Playlists in the folders](docs/playlists.md)        | `aede playlist`                                                                      |
| [Formats and dependencies](docs/formats.md)          | What is read, by which parser, and what is depended on                               |

**Why it is built this way**

The design notes are kept because the reasoning is worth more than the result:
most of them exist to explain a refusal.

| Page                                                               | What is in it                                                                   |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| [Architecture](docs/design/architecture.md)                        | The graph model, the crates, the tooling, the tests                             |
| [What the user writes](docs/design/annotations.md)                 | Why annotations live in a file of their own, and the identity problem behind it |
| [The attributed layer (M1.0)](docs/design/attribution.md)          | Where a fetched value is kept, and why it never lands on top of a tag           |
| [Querying](docs/design/querying.md)                                | Why a query language is an interface and not a storage engine                   |
| [Playback (M3)](docs/design/playback.md)                           | The queue, shuffle, loudness, gapless                                           |
| [Identification (M1)](docs/design/identification.md)               | MusicBrainz, editions, band membership, what is missing from the shelf          |
| [Lyrics](docs/design/lyrics.md)                                    | Three problems that share a word                                                |
| [Paths](docs/design/paths.md)                                      | Why a catalog path is a `/`-separated string, and why Windows is not published  |
| [Plugins, if there are any](docs/design/plugins.md)                | Why a plugin would be a program and not a library, and what it could not be     |
| [Speaking other tools' languages](docs/design/interoperability.md) | Beets, MPD, Picard: what is borrowed and what is refused                        |
| [Roadmap](docs/design/roadmap.md)                                  | M0 to M3, and what is deliberately left out                                     |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The short version: the mechanics are
GitHub's and need no explaining, but what this project accepts and refuses does
— a pull request adding a crate, rewriting tags, or reaching the network is one
somebody spent an evening on for nothing.

## Licence

MIT — see [LICENSE](LICENSE).
