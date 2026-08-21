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
| `aede check [folder…]`                                    | Verify the checksums the files carry (`--full` re-verifies everything)                   |
| `aede artists` / `albums` / `genres` / `labels` / `years` | Listings                                                                                 |
| `aede artist "<name>"`                                    | Discography, collaborations, roles (`--with <other>` lists the tracks two artists share) |
| `aede album "<title>"`                                    | Tracks, durations, formats, credits                                                      |
| `aede track "<title>"`                                    | Every track carrying this title: album, credits, technical details, tags                 |
| `aede search <text>`                                      | Search across the whole catalog                                                          |
| `aede file <path>`                                        | Inspect a single file, outside the catalog                                               |
| `aede export`                                             | Export the catalog as JSON, or as CSV with `--csv`                                       |
| `aede import <report…>`                                   | Take in a FlacCompagnon report (`--forget` removes what was imported)                    |
| `aede reset`                                              | Remove the catalog, after confirmation (`--yes` skips it)                                |

`--json` on `stats`, `doctor`, `search` and `track` produces machine-readable output. `aede help` lists every option.

The catalog lives in `$AEDE_HOME`, or `~/.local/share/aede/catalog.json`.

### Getting the data out

Three formats, because they answer three different questions.

**JSON** (`aede export`) is the faithful dump: ten linked tables, one per table of the model. It is what rebuilds a catalog or feeds another program.

**CSV** (`aede export --csv`) is for a spreadsheet, and a spreadsheet cannot hold a graph. It writes **one row per album** — artist, title, year, track and disc counts, duration, size, formats, sample rates, bit depths, label, catalogue number, genres, integrity, folder — which is the view from above: sort by size to find what to re-rip, filter on `lossless` to see what is left to replace. `--tracks` switches to one row per track when the album is too coarse.

Its values are **raw**: `duration_ms` and `size_bytes`, not `4:20` and `31.2 MB`. A column that cannot be added up is a column that cannot be used.

`--separator=;` for Excel in a French or German locale, `--separator=tab` for a TSV. Fields are quoted per RFC 4180, so a title carrying a comma or a quotation mark does not shift every column that follows.

**M3U** (`--m3u`) is not an export of the catalog but of a **selection**: whatever is on screen becomes a playlist.

```sh
aede album "To Hell With God" --m3u --output=deicide.m3u8
aede search coltrane --m3u --output=coltrane.m3u8
mpv --playlist=deicide.m3u8
```

Paths are absolute, so the playlist works wherever it is saved; `#EXTINF` carries the duration and the title, so a player shows them without opening every file. Without `--output` it goes to standard output, which a shell supporting process substitution can hand straight to a player — `mpv --playlist=<(aede artist "Ozzy Osbourne" --m3u)`.

### Where each option applies

Three groups, and an option that a command cannot honour is **refused**, never ignored.

`export` describes the **catalog**: `--csv` gives one row per album, `--tracks` one row per track. It takes no argument.

The **listings** — `albums`, `artists`, `genres`, `labels`, `years` — turn into a table of exactly what they show, filters included. This is how several albums land in one file:

```sh
aede albums --csv --artist="Deicide" --output=deicide.csv
aede albums --csv --year=1990
aede artists --csv --limit=100 --output=artists.csv
```

`album`, `artist`, `track` and `search` describe a **selection**: `--csv` and `--m3u` both apply to it, as a table of tracks or as a playlist. For an artist, that means the tracks they are audible on; for a search, the track hits and not the artists or albums found.

```sh
aede album "To Hell With God" --csv --output=album.csv
aede artist "Deicide" --csv | sort -t, -k9 -n     # sorted by size
```

`aede album` takes **one** title — the words are joined so a title can be typed without quotes — and says which command lists several when given more.

The same holds for an option whose value is a **name**: `--artist`, `--album`, `--with`, `--genre` and `--label` take the words that follow, up to the next option. `--limit`, `--year`, `--output` and the rest take exactly one word, because a number or a path is one word.

```sh
aede artist Ozzy --with Zakk Wylde        # no quotes needed anywhere
aede artist Ozzy --with "Zakk Wylde"      # the same thing
aede track So What --artist Miles Davis --limit 1
```

Put the positional before the option: `aede track --artist Miles Davis So What` gives the whole tail to `--artist`, and the command then says it was given no title — rather than answering a question you did not ask.

`--output <file>` writes wherever these produce text, and states where it went instead of filling the terminal.

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

| Line                      | What it counts                                                                                                      |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Files found               | Audio files seen while walking the folders, duplicates removed                                                      |
| Read from disk            | Files whose tags were parsed: new ones, and those changed since the last scan                                       |
| Reused from previous scan | Files identical in path, size and modification time; their tags came from the catalog, untouched on disk            |
| Gone since last scan      | Files the catalog knew and that are no longer there; they leave the catalog                                         |
| Analyses imported         | [FlacCompagnon reports](#what-another-tool-found) found in the folders and taken in; only shown when there were any |
| Analyses now attached     | Imported analyses that were waiting for a file and found it this time                                               |
| Elapsed                   | Wall-clock time of the whole scan, folder walk included                                                             |

`Files found` is always the sum of the two middle lines. A file that could not be read is listed underneath with the reason, and stays out of the catalog without stopping the scan.

### Starting over

`aede reset` removes the catalog. It first says what it holds — tracks, albums, artists, watched folders, integrity verdicts, imported analyses — and what a rescan does not bring back:

```
$ aede reset

About to remove the catalog

  Tracks                  20 148
  Watched folders              2
  Integrity verdicts      20 148
  Imported analyses          312
  File                    9.4 MB
  a scan rebuilds the catalog; the watched folders and the integrity
  verdicts are lost and have to be redone
  the imported analyses go too, and have to be imported again
  Type "yes" to confirm:
```

`--yes` skips the question, for scripts and tests. Without a terminal to ask on, the command **refuses** rather than assuming an answer either way. Once done, it prints the `aede scan` that rebuilds what it removed, watched folders included — that line is the only trace left of them.

## Are the files still intact?

`aede check` answers the one question the tags cannot: has the audio been damaged since it was written? It reads no reference copy and decodes nothing — it verifies the checksums the containers already carry.

| Container                 | What is verified                                              |
| ------------------------- | ------------------------------------------------------------- |
| FLAC                      | The CRC-16 of every frame and the CRC-8 of every frame header |
| Ogg (Vorbis, Opus, Speex) | The CRC-32 of every page                                      |
| MP3, MP4, WAV, AIFF       | Nothing: these formats carry no checksum                      |

That catches what actually happens to stored files — a flipped bit, a bad sector, a truncated copy. A truncated file is caught even though every frame it still holds is valid, because the last one has to end where the file does.

Four states, and the fourth is the one usually forgotten:

- **not verified** — no check has been run on this file yet;
- **nothing to check** — the container carries no checksum, and no amount of re-running will change that;
- **intact** — every checksum matched;
- **damaged** — one did not, with the frame or page named.

The verdict is stored per file and survives across scans, so the cost is paid once: a second `aede check` has nothing to read. A file that changed loses its verdict, since it is no longer the file that was verified. `doctor` reports damage as an error and says how many files have never been verified rather than letting a library look healthy.

### How long it takes, and how to start small

Verifying means **reading every byte** of the files concerned. On a library of 20 000 tracks — some 600 GB — that is a few minutes on an NVMe drive, and it can be well over an hour on a mechanical disk or a NAS. The time is spent on input/output, not on computation, so more cores barely help.

That is why the check is opt-in, and why it announces itself before starting:

```
$ aede check
Verifying 20 148 files to read, 612.4 GB
  this reads every byte: minutes on an SSD, longer on a mechanical disk
  stopping it is safe — verified files are saved as the run goes
```

Two things make it manageable:

**Start on a corner.** `aede check ~/Music/Deicide` restricts the run to a folder, as many as you like. Useful for a first look, and for re-verifying a drive you suspect without touching the rest.

**Interrupting is safe.** Verdicts are written to the catalog every 250 files rather than at the end, so a `Ctrl-C` — or a laptop closing, or a drive going away — costs at most the batch in progress. Everything already verified is kept, and the next run picks up exactly where the last one stopped, since a file that has a verdict is no longer in the queue. A second full run therefore has nothing left to read.

What this does **not** prove is that the audio itself is untouched — a stream re-encoded consistently would pass. FLAC also stores an MD5 of the _decoded_ audio, and checking it means decoding; that verdict arrives with the playback engine at M3, and the stored shape already accommodates it. Until then, [taking in another tool's analysis](#what-another-tool-found) fills the gap for whoever already has one.

## What another tool found

Entirely optional, and it changes nothing if you never use it.

Aède reads the _structure_ of a file. It does not decode, so there are questions it cannot answer yet: is this FLAC a re-encoded MP3, was it upsampled, where does the spectrum stop, how loud is it really — and the decisive one, does the decoded audio still match the MD5 the encoder wrote into the file.

[FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) already does that pass. If you have run it, `aede import` puts the results into the catalog:

```sh
aede import ~/Desktop/danzig-report.json
aede import ~/Desktop/reports/            # every .json underneath, at any depth
aede import --forget                      # remove them all
aede import --forget --source=flaccompagnon
```

A folder is walked **recursively**, because reports are kept the way the albums they describe are: one folder per artist, one per album.

### The order does not matter

An analysis is filed under the **path** it describes, not under a catalog entry. So the two operations can be done either way round, which matters because analysing a folder and _then_ building the library from it is the natural order for someone who already owns the other tool.

- **Import first.** The records are stored and reported as `Waiting for a scan`. The scan that brings those files in makes them attach by themselves, and says so (`Analyses now attached`). `doctor` says how many are still waiting rather than letting them sit there unmentioned.
- **Scan first.** Files are matched by path, then by name and size for a library that has moved since — a name and a byte count together are very nearly unique.
- **Leave the report in the album folder.** A scan walks over it anyway: any `.json` announcing itself as a FlacCompagnon report is read and taken in, and the scan report says how many. Half a kilobyte is read from each `.json` met to recognise one, so nothing else in the library is parsed.

Matching never relies on the two paths being written the same way. Watched folders are stored canonical, so a report produced against a symbolic link — or against `/var` where macOS says `/private/var` — names the very same file by a string that will never compare equal; the name and the size bridge the two, and the record is then refiled under the path the catalog uses.

Being _about_ a file is not the same as _describing_ it: a record that matches by name and size is still checked against that file's modification date, and dropped if the file was written to since.

Imported analyses survive a scan — they are the one thing in the catalog that reading the files again cannot recompute.

`aede track` then shows a second panel, named after whoever measured it:

```
Analysed by flaccompagnon

  MD5              Match
  Real bit depth   16 bits
  Cutoff           22.1 kHz
  Dynamic range    9.3 dB
  True peak        0.28 dBTP
  Verdict          Clean — full-band content to ~22.1 kHz, no lossy signature.
```

Three rules govern what happens to those numbers.

**They are never merged into Aède's own.** A verdict carries the method that produced it. Overwriting the bit depth read from the frames with one obtained by decoding would leave the catalog unable to say where the number came from — and unable to notice that the two disagree. Noticing is the point.

**They expire with the bytes they describe.** An analysis is bound to the size and modification date the file had when it was measured, the same test the incremental scan uses. Edit the file and the panel says `— stale: the file changed since` rather than answering confidently about music that is no longer there. Re-importing that same report is refused for the same reason.

**A disagreement is a finding, not something to arbitrate.** `doctor` reports an MD5 mismatch as an **error** even when `aede check` found the file intact, because the two look at different things:

```
error  audio does not match its MD5
       flaccompagnon decoded the audio and it does not match the file's own MD5,
       although the frame checksums are valid: the stream was re-encoded
```

The frame checksums prove the _container_ was not corrupted; the MD5 proves the _audio_ is the audio that was encoded. A file passes the first and fails the second when it was re-encoded by a tool that rewrote the frames but kept the old signature — exactly the case Aède cannot see before it decodes anything itself. A lossy ancestry — transcoded, upscaled, upsampled — is reported as a warning, with the reason and the source named.

### Where it is all stored

In the catalog, and nowhere else: `~/.local/share/aede/catalog.json` grows one more table, `analysis`, one row per path and per source. The report you imported is never referred to again — you can move it or throw it away. `aede export` includes the table, `aede import --forget` empties it, and `aede reset` warns about it before removing the catalog.

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
    src/text.rs       name normalization (how entities are matched)
    src/json.rs       minimal JSON reader and writer
    src/clock.rs      the one unit of time the catalog stores
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

Formatting is `rustfmt` (`rustfmt.toml`); Prettier only covers Markdown, JSON and YAML (`.prettierrc`). The project targets **zero clippy warnings**.

## Tests

```sh
cargo test
```

199 tests: binary parsers (including truncated files and forged signatures), name normalization, graph construction, persistence round-trip, statistics, diagnostics, table alignment, argument parsing, and an end-to-end test that runs the binary.

## Roadmap

**M0 — the catalog (this repository).** Scanning, graph model, statistics, diagnostics, command-line navigation.

**M1 — identification.** MusicBrainz for relations and credits, AcoustID/Chromaprint for badly tagged files, Cover Art Archive for artwork, Wikidata to reach the Wikipedia article in the user's language — in the vast majority of cases it already exists, written by humans, so no machine translation is needed. Move to SQLite. The hard part will be matching files to releases: plan for a confidence score and manual correction, never a blind rewrite.

**M2 — the API.** HTTP server, JSON and WebSocket. To be frozen early: it is the contract between the core and every future client. This is where `serde` becomes worth its place: hand-written serialization is fine for one internal format, but not for an HTTP contract with a dozen types on it. The current `json` module was written for the catalog file, and the move is meant to be mechanical.

**M3 — playback.** Local output, queue, gapless playback, loudness normalization (EBU R128). The decoder written for it also brings the FLAC MD5 check, which verifies the decoded audio rather than the container.

**M4 — the network.** Remote playback endpoints. `slimproto` gives the best effort-to-result ratio, since it opens up a fleet of existing devices without reinventing anything; UPnP/OpenHome afterwards for commercial hi-fi streamers.

Explicitly out of scope: RAAT and the "Roon Ready" certification are proprietary and licensed — there is no technical path to them.

## Licence

MIT — see [LICENSE](LICENSE).
