# Aède

A local music library, written in Rust.

An _aède_ (Greek ἀοιδός, _aoidos_) was the poet-singer of archaic Greece: he held the whole repertoire in memory and performed it. Keeping and playing, in one word — which is exactly what this program is for.

This repository is **milestone M0**: read folders, turn them into a catalog of linked entities, and answer questions about it. No audio playback and no network access yet — that is deliberate, see the roadmap at the end.

## Getting started

```sh
cargo build --release
./target/release/aede scan ~/Music
./target/release/aede stats
```

Rust 1.89 or later. The build downloads one dependency, `lofty`; everything after the first build works offline.

## Commands

| Command                                                   | What it does                                                                             |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `aede scan [folder…]`                                     | Scan the watched folders; any folder given is added to them                              |
| `aede roots`                                              | List the watched folders (`--remove <folder>` to drop one)                               |
| `aede stats`                                              | Tracks, albums, formats, quality, decades, completeness                                  |
| `aede doctor`                                             | Missing tags, duplicates, incomplete albums, mixed formats                               |
| `aede artists` / `albums` / `genres` / `labels` / `years` | Listings                                                                                 |
| `aede artist "<name>"`                                    | Discography, collaborations, roles (`--with <other>` lists the tracks two artists share) |
| `aede album "<title>"`                                    | Tracks, durations, formats, credits                                                      |
| `aede track "<title>"`                                    | Every track carrying this title: album, credits, technical details, tags                 |
| `aede search <text>`                                      | Search across the whole catalog                                                          |
| `aede file <path>`                                        | Inspect a single file, outside the catalog                                               |
| `aede export`                                             | Export the catalog as JSON                                                               |

`--json` on `stats`, `doctor`, `search` and `track` produces machine-readable output. `aede help` lists every option.

The catalog lives in `$AEDE_HOME`, or `~/.local/share/aede/catalog.json`.

```
$ aede stats

Library

  Tracks                        20
  Albums                         6
    of which compilations        1
  Artists                        8
  Total duration              38 s
  Size on disk              1.3 MB

Quality

                       Count      Size
  ───────────────────  ─────  ────────  ────────────────────
  Lossless (CD)           11  399.3 kB  ████████████████████
  Hi-res                   4  664.3 kB  ███████·············
  Lossy (>= 256 kbps)      3  248.3 kB  █████···············
```

### Trying it without a library at hand

```sh
tools/demo-library.sh /tmp/demo-music   # requires ffmpeg
aede scan /tmp/demo-music
aede doctor
```

The demo library is deliberately damaged: untagged files, a duplicate, an album missing a track, an album with mixed formats. Enough for `doctor` to have something to bite on.

### Reading the scan report

| Line                      | What it counts                                                                                           |
| ------------------------- | -------------------------------------------------------------------------------------------------------- |
| Files found               | Audio files seen while walking the folders, duplicates removed                                           |
| Read from disk            | Files whose tags were parsed: new ones, and those changed since the last scan                            |
| Reused from previous scan | Files identical in path, size and modification time; their tags came from the catalog, untouched on disk |
| Gone since last scan      | Files the catalog knew and that are no longer there; they leave the catalog                              |
| Elapsed                   | Wall-clock time of the whole scan, folder walk included                                                  |

`Files found` is always the sum of the two middle lines. A file that could not be read is listed underneath with the reason, and stays out of the catalog without stopping the scan.

## Supported formats

| Container   | Codecs                    | Tags                          | Duration from                  |
| ----------- | ------------------------- | ----------------------------- | ------------------------------ |
| FLAC        | FLAC                      | Vorbis Comment, leading ID3v2 | STREAMINFO                     |
| MP3         | MPEG 1/2/2.5 layers I–III | ID3v2.2/2.3/2.4, ID3v1        | Xing / VBRI / constant bitrate |
| MP4         | ALAC, AAC                 | iTunes atoms, freeform `----` | `mvhd`                         |
| Ogg         | Vorbis, Opus              | Vorbis Comment                | Granule position               |
| WAV         | PCM                       | `LIST/INFO`, `id3 ` chunk     | `fmt ` + `data`                |
| AIFF / AIFC | PCM                       | `NAME`/`AUTH`, `ID3 ` chunk   | `COMM`                         |

Extensions: `.flac` `.mp3` `.m4a` `.m4b` `.mp4` `.alac` `.ogg` `.oga` `.opus` `.wav` `.wave` `.aif` `.aiff` `.aifc`

The formats below are read through [`lofty`](https://crates.io/crates/lofty), which takes over whenever the signature matches none of the parsers above:

| Container      | Codecs           | Tags           | Duration from      |
| -------------- | ---------------- | -------------- | ------------------ |
| AAC            | AAC              | ID3v2, ID3v1   | ADTS frame headers |
| WavPack        | WavPack          | APEv2, ID3v1   | Block headers      |
| Monkey's Audio | APE              | APEv2, ID3v1   | Descriptor         |
| Musepack       | Musepack SV7/SV8 | APEv2, ID3v1   | Stream header      |
| Speex          | Speex            | Vorbis Comment | Granule position   |

Extensions: `.aac` `.ape` `.wv` `.mpc` `.mp+` `.mpp` `.spx`

The fallback is only ever reached last, so it can never take a format away from a parser above.

The parsers are written by hand from the specifications and validated against real files produced by ffmpeg and cross-checked with ffprobe (`crates/aede-core/tests/real_files.rs`). The awkward cases are covered: UTF-16 with a BOM, ID3v2.3 unsynchronisation, numeric genres like `(17)Rock`, the LAME encoder delay (needed later for gapless playback), the ALAC magic cookie for the real bit depth, the Opus pre-skip.

No `unwrap` and no direct indexing anywhere in the parsers: a truncated file yields an error or a partial result, never a panic. A test checks this by truncating a real file to a quarter, a third and a half of its size.

## Dependencies

One: [`lofty`](https://crates.io/crates/lofty), which reads the formats listed above as its own. A dependency is a requirement here, not a dogma — it has to do something we could not do as well ourselves, be maintained and widely used, and bring a dependency tree small enough to read. `lofty` earns its place by covering a long tail of formats that would each take days to parse and would be exercised by a handful of files.

What it does not replace: the encoder delay and padding of the LAME tag, the ALAC magic cookie and the Opus pre-skip are not exposed by any general-purpose library, and milestone M3 needs them for gapless playback. That is why the main formats keep their own parsers.

## Architecture

```
crates/
  aede-core/        library, no terminal I/O
    src/tags/         one parser per format -> RawTags
      foreign.rs        the formats handed to lofty
    src/audit/        what a file contains, as opposed to what it claims
    src/model.rs      the graph: entities, credits, relations
    src/scan.rs       directory walk, parallel reads, incremental
    src/store.rs      JSON persistence, atomic writes
    src/stats.rs      statistics
    src/doctor.rs     diagnostics
    src/text.rs       name normalization (how entities are matched)
    src/json.rs       minimal JSON reader and writer
    schema.sql        the SQLite schema targeted by milestone M1
  aede-cli/         the `aede` binary
    src/commands/     one module per group of commands
tools/              development scripts
CLAUDE.md           project conventions and invariants
```

### The decision that shapes everything

The model is not "an album belongs to an artist" but **entities carrying roles towards one another**. Two tables do all the work:

- `credit` — _who does what, on what_: performer, composer, conductor, producer, engineer…
- `relation` — _typed links between entities_, with a weight and a source.

This is what will let you click a drummer and see their forty appearances. A flat schema would have to be thrown away the day MusicBrainz arrives.

The graph is already usable at M0, with no network access at all: `aede artist "Queen"` shows "played with David Bowie", inferred purely from the fact that both are credited on the same track. `aede artist "Queen" --with "David Bowie"` then lists those tracks. The relation itself stores only a count; the tracks behind it are recomputed from the `credit` table on demand, so a corrected tag never leaves a stale list behind.

### A few choices worth knowing about

**The catalog is a JSON file whose every key is a table.** Not a stopgap: it mirrors `schema.sql` exactly. Moving to SQLite means rewriting `store.rs` and nothing else.

**Raw tags are kept per file.** The graph can therefore be rebuilt entirely without touching the disk, which is what makes the incremental scan possible and will allow undoing an automatic correction.

**Scanning is incremental by default.** Same path, size and modification time means the file is not read again. On the demo library: 63 ms on the first pass, 2 ms on the second. `--full` forces a re-read.

**Watched folders accumulate.** `aede scan ~/Music` then `aede scan ~/Live` watches both; a plain `aede scan` refreshes everything. `--replace` keeps only the folders given, and `aede roots --remove` drops one. A watched folder that has become unreachable aborts the scan rather than quietly emptying the catalog.

Dropping a folder from the list does not empty the catalog on its own: its files stay until the next `aede scan` rebuilds the catalog from the folders still watched. Run that scan **without naming a folder** — naming the one just dropped would simply watch it again.

**A row measures what it counts.** In _Appears on_, the duration and the size cover the tracks the artist is on, not the whole album: one guest song is one guest song, not forty minutes of somebody else's record.

**A guest appearance is not part of a discography.** Singing one track on somebody else's album puts it under _Appears on_, never under _Discography_, and writing or production credits go in a third section. The performer rankings count performing credits only.

**A title is not an identifier.** `aede track "So What"` prints every track carrying that name — the studio take, the single, the live rendition — because they are different recordings. `--artist` and `--album` narrow it down, and a list cut short by `--limit` always says so.

**Construction is deterministic.** Files are sorted before processing, so two scans of the same library produce exactly the same identifiers. Without that, no readable diff and no reproducible test.

**`Various Artists` is not an artist**, it is the absence of an album artist. Recording it would pollute every count.

**Sizes are decimal, durations are rounded.** 1 kB is 1000 bytes, the convention macOS Finder and most Linux file managers use, so a figure here matches what the system says about the same files. A duration is rounded to the nearest second rather than truncated, as players do.

**Every listing carries the same measures.** Count, duration and size on disk appear on each entity page and in each listing: what a slice of the library weighs should not depend on the command used to look at it.

**An artist is counted once per track.** Miles Davis as both performer and composer of "So What" is one track, not two.

## Tooling

```sh
tools/check.sh        # formatting, lint, tests, release build
cargo doc --no-deps --open   # the API documentation
```

Every public item of `aede-core` is documented: the crate sets `#![warn(missing_docs)]`, so a gap is a warning and `tools/check.sh` fails on it.

Formatting is `rustfmt` (`rustfmt.toml`); Prettier only covers Markdown, JSON and YAML (`.prettierrc`). The project targets **zero clippy warnings**.

## Tests

```sh
cargo test
```

146 tests: binary parsers (including truncated files and forged signatures), name normalization, graph construction, persistence round-trip, statistics, diagnostics, table alignment, argument parsing, and an end-to-end test that runs the binary.

## Roadmap

**M0 — the catalog (this repository).** Scanning, graph model, statistics, diagnostics, command-line navigation.

**M1 — identification.** MusicBrainz for relations and credits, AcoustID/Chromaprint for badly tagged files, Cover Art Archive for artwork, Wikidata to reach the Wikipedia article in the user's language — in the vast majority of cases it already exists, written by humans, so no machine translation is needed. Move to SQLite. The hard part will be matching files to releases: plan for a confidence score and manual correction, never a blind rewrite.

**M2 — the API.** HTTP server, JSON and WebSocket. To be frozen early: it is the contract between the core and every future client. This is where `serde` becomes worth its place: hand-written serialization is fine for one internal format, but not for an HTTP contract with a dozen types on it. The current `json` module was written for the catalog file, and the move is meant to be mechanical.

**M3 — playback.** Local output, queue, gapless playback, loudness normalization (EBU R128).

**M4 — the network.** Remote playback endpoints. `slimproto` gives the best effort-to-result ratio, since it opens up a fleet of existing devices without reinventing anything; UPnP/OpenHome afterwards for commercial hi-fi streamers.

Explicitly out of scope: RAAT and the "Roon Ready" certification are proprietary and licensed — there is no technical path to them.

## Licence

MIT — see [LICENSE](LICENSE).
