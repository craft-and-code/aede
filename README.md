# Aède

A local music library, written in Rust.

An _aède_ (Greek ἀοιδός, _aoidos_) was the poet-singer of archaic Greece: he held the whole repertoire in memory and performed it. Keeping and playing, in one word — which is exactly what this program is for.

This repository is **milestone M0.5**: read folders, turn them into a catalog of linked entities, answer questions about it, and keep what you think of it. No audio playback and no network access yet — that is deliberate, and the [roadmap](docs/design/roadmap.md) says when each arrives.

## Getting started

```sh
cargo build --release
./target/release/aede scan ~/Music
./target/release/aede stats
```

Rust 1.89 or later. The build downloads one dependency, `lofty`; everything after the first build works offline.

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
| `aede search <text>`                                      | Search across the whole catalog (`--comments` looks in the comment tag, `--notes` in your own notes)                                                                           |
| `aede spectrum [folder…]`                                 | Draw a spectrogram of every track into a `spectrograms/` folder beside it, through ffmpeg                                                                                      |
| `aede stats`                                              | Tracks, albums, formats, quality, decades, completeness                                                                                                                        |
| `aede track "<title>"`                                    | Every track carrying this title: album, credits, technical details, tags                                                                                                       |

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
| [Copying to a player](docs/copying.md)               | `aede copy`, companion files, safe names, encoding on the way out                    |
| [Spectrograms](docs/spectrograms.md)                 | `aede spectrum`                                                                      |
| [Playlists in the folders](docs/playlists.md)        | `aede playlist`                                                                      |
| [Formats and dependencies](docs/formats.md)          | What is read, by which parser, and the one dependency                                |

**Why it is built this way**

The design notes are kept because the reasoning is worth more than the result:
most of them exist to explain a refusal.

| Page                                                               | What is in it                                                                   |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| [Architecture](docs/design/architecture.md)                        | The graph model, the crates, the tooling, the tests                             |
| [What the user writes](docs/design/annotations.md)                 | Why annotations live in a file of their own, and the identity problem behind it |
| [Querying](docs/design/querying.md)                                | Why a query language is an interface and not a storage engine                   |
| [Playback (M3)](docs/design/playback.md)                           | The queue, shuffle, loudness, gapless                                           |
| [Identification (M1)](docs/design/identification.md)               | MusicBrainz, editions, band membership, what is missing from the shelf          |
| [Lyrics](docs/design/lyrics.md)                                    | Three problems that share a word                                                |
| [Speaking other tools' languages](docs/design/interoperability.md) | Beets, MPD, Picard: what is borrowed and what is refused                        |
| [Roadmap](docs/design/roadmap.md)                                  | M0 to M3, and what is deliberately left out                                     |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The short version: the mechanics are
GitHub's and need no explaining, but what this project accepts and refuses does
— a pull request adding a crate, rewriting tags, or reaching the network is one
somebody spent an evening on for nothing.

## Licence

MIT — see [LICENSE](LICENSE).
