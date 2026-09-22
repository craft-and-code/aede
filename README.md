# Aède — Archival Music Library Manager

> _A digital sanctuary for serious music collectors, archivists, and audio curators._

**Aède** is a high-precision, read-only local music library manager and cataloging system written in Rust. Designed with an uncompromising commitment to archival integrity, Aède treats your master music collection as a sanctuary: it reads metadata, verifies audio container integrity, indexes complex credit graphs, and generates derivative assets—**without ever writing a single byte back into your original audio files**.

> [!TIP]
> An _aède_ (Greek ἀοιδός, _aoidos_) was the poet-singer of archaic Greece: he held the whole repertoire in memory and performed it. Keeping and playing, in one word — which is exactly what this program is for.

---

## 🏛️ Design Philosophy & Core Principles

1. **Vault Sanctity (Strict Non-Destructive Read-Only Storage)**  
   Aède never mutates, re-tags, or re-organizes the files inside your watched music directories. Tags and file names inside your library remain untouched. All annotations, user tags, play counts, manual merges, and query collections reside safely in a separate local state store (`user.json`).
2. **Bespoke Forensic Parsers**  
   The primary audio containers (FLAC, MP3, MP4/ALAC, Ogg Vorbis/Opus, WAV, AIFF) are parsed by native Rust code implemented directly from container format specifications. Every parser guarantees zero `unwrap()` calls and zero direct memory indexing—a corrupted or violently truncated file produces an explicit diagnostic error rather than a panic.
3. **Sample-Accurate Precision**  
   General-purpose tagging libraries frequently ignore crucial playback metadata. Aède extracts LAME encoder delay and padding, ALAC magic cookies, and Opus pre-skip samples, ensuring the exact foundation required for sample-accurate, gapless audio playback.
4. **Empirical & Deterministic Operations**  
   Aède shuns silent heuristics and hidden fallbacks. A query against a non-existent genre returns an explicit error rather than a deceptively empty list. Destination filesystems during transfers (`aede copy`) are probed empirically by writing invisible test files rather than relying on brittle OS lookup tables.
5. **Separation of Fact and Inference**  
   Container integrity checks (`aede check`) verify mathematical frame and page checksums ($CRC\text{-}8$, $CRC\text{-}16$, $CRC\text{-}32$). External spectral analyses (`aede import`) measure physical acoustic metrics. Aède keeps container facts separate from acoustic inferences, preserving data provenance across all commands.

---

## 🛠️ System Architecture & Audio Parsers

Aède uses a two-tier parsing architecture. Mainstream, high-fidelity containers are parsed natively by custom, zero-panic Rust engines. Niche and legacy archival formats fall back gracefully to the audited `lofty` crate.

| Container          | Codecs                    | Tag Standards                   | Duration Source               | Parser Tier          |
| :----------------- | :------------------------ | :------------------------------ | :---------------------------- | :------------------- |
| **FLAC**           | FLAC                      | Vorbis Comment, ID3v2 Header    | `STREAMINFO` block            | Native (`aede-core`) |
| **MP3**            | MPEG 1/2/2.5 Layers I–III | ID3v2.2/2.3/2.4, ID3v1          | Xing / VBRI / CBR calculation | Native (`aede-core`) |
| **MP4 / M4A**      | ALAC, AAC                 | iTunes Atoms, Freeform `----`   | `mvhd` / `mdhd` atom          | Native (`aede-core`) |
| **Ogg**            | Vorbis, Opus              | Vorbis Comment                  | Granule position              | Native (`aede-core`) |
| **WAV**            | PCM                       | `LIST/INFO` chunk, `id3 ` chunk | `fmt ` + `data` chunk sizes   | Native (`aede-core`) |
| **AIFF / AIFC**    | PCM                       | `NAME`/`AUTH`, `ID3 ` chunk     | `COMM` chunk                  | Native (`aede-core`) |
| **AAC**            | AAC                       | ID3v2, ID3v1                    | ADTS frame headers            | Fallback (`lofty`)   |
| **WavPack**        | WavPack                   | APEv2, ID3v1                    | Block headers                 | Fallback (`lofty`)   |
| **Monkey's Audio** | APE                       | APEv2, ID3v1                    | Descriptor headers            | Fallback (`lofty`)   |
| **Musepack**       | Musepack SV7/SV8          | APEv2, ID3v1                    | Stream headers                | Fallback (`lofty`)   |
| **Speex**          | Speex                     | Vorbis Comment                  | Granule position              | Fallback (`lofty`)   |

---

## 📦 Installation & System Dependencies

### Prerequisites

- **Rust Toolchain:** Stable Rust compiler (1.75+) and `cargo`.
- **FFmpeg (Optional, Recommended):** Required for on-the-fly transcoding (`aede copy --compress`) and acoustic spectrogram generation (`aede spectrum`).

### System Dependencies

# macOS (Homebrew)

```sh
brew install rust ffmpeg
```

# Debian / Ubuntu

```sh
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev ffmpeg
```

# Arch Linux

```sh
sudo pacman -S base-devel rust ffmpeg
```

### Building & Installing from Source

```sh
# Clone the repository
git clone https://github.com/your-org/aede.git
cd aede

# Build and install the binary
cargo install --path crates/aede-cli
```

---

## 🚀 Quick Start Guide

```sh
# 1. Register your music directories (watched roots)
aede roots ~/Music/FLAC /Volumes/AudioArchive

# 2. Perform an initial library scan
aede scan

# 3. Perform a container integrity audit (detect bit rot)
aede check

# 4. Search your collection using relational queries
aede query "artist:Coltrane year:1959..1965 lossless:true"

# 5. Export a curated selection for a portable player with MP3 conversion
aede copy /Volumes/DAP --query "loved rating:>=4" --compress mp3 --quality V0

# 6. Check library health and metadata anomalies
aede doctor
```

---

## 📋 Complete Technical Command Reference

### Core Vault & Catalog Management

| Command       | Arguments    | Key Options                                 | Description                                                                                 |
| :------------ | :----------- | :------------------------------------------ | :------------------------------------------------------------------------------------------ |
| `aede roots`  | `[paths...]` | `--exclude <path>`, `--remove`, `--no-scan` | Display, add, or exclude watched storage directories.                                       |
| `aede scan`   | `[path]`     | `--full`                                    | Traverses roots to index audio files, tags, and structure.                                  |
| `aede check`  | `[path]`     | `--full`                                    | Audits frame/page checksums ($CRC\text{-}8$, $CRC\text{-}16$, $CRC\text{-}32$) for bit rot. |
| `aede doctor` | None         | None                                        | Run a health check: metadata, duplicates, source conflicts and incomplete credits.         |
| `aede review` | None         | `--interactive`, `--accept=<ID>`, `--reject=<ID>`, `--undo=<ID>`, `--all` | Resolve uncertain source identities without rewriting tags.               |
| `aede stats`  | None         | None                                        | Displays catalog metrics, audio quality distribution, and credit roles.                     |
| `aede reset`  | None         | `--yes`                                     | Wipes indexed catalog data while preserving root configurations.                            |

### Query, Search & Catalog Browsing

| Command          | Arguments       | Key Options                                                                                                               | Description                                                                          |
| :--------------- | :-------------- | :------------------------------------------------------------------------------------------------------------------------ | :----------------------------------------------------------------------------------- |
| `aede query`     | `<expression>`  | `--m3u`, `--csv`, `--json`                                                                                                | Evaluates a relational search expression across the catalog graph.                   |
| `aede search`    | `<term>`        | `--comments`, `--notes`, `--lyrics`                                                                                       | Search artists, albums, tracks, recordings, works, release groups and optional prose. |
| `aede albums`    | None            | `--artist`, `--genre`, `--year`, `--compilations`, `--no-compilations`, `--limit`, `--offset`, `--all`, `--csv`, `--json` | List and filter album records with pagination support.                               |
| `aede artists`   | None            | `--role <role>`, `--country <code>`, `--limit`, `--offset`, `--all`, `--csv`, `--json`                                    | List artists, filter by credit role, or map by geographic origin.                    |
| `aede genres`    | `[name]`        | `--m3u`, `--csv`, `--json`                                                                                                | Browse music genres or export tracks matching a specific genre.                      |
| `aede labels`    | `[name]`        | `--m3u`, `--csv`, `--json`                                                                                                | Survey record imprints and catalog releases.                                         |
| `aede artist`    | `<name>`        | `--with <artist>`, `--members`, `--m3u`, `--csv`                                                                          | Show one artist's albums, collaborations, credits, and relationships.                |
| `aede album`     | `<title\|MBID>` | `--m3u`, `--csv`                                                                                                          | Show one local edition, its tracks, credits, and graph links.                        |
| `aede track`     | `<title>`       | `--artist`, `--lyrics`, `--limit`                                                                                         | Show a local placement, tags, credits, and technical facts.                          |
| `aede recording` | `<title\|MBID>` | None                                                                                                                      | Show a recorded performance, every local placement, and sourced works.               |
| `aede work`      | `<title\|MBID>` | None                                                                                                                      | Show a composition and its canonical or source-backed recordings.                    |
| `aede release-group` | `<title\|MBID>` | None                                                                                                                   | Show the album identity shared by every local edition.                               |
| `aede label`     | `<name>`        | `--m3u`, `--csv`                                                                                                          | Show a label catalog and its explicit, confirmed, proposed, or conflicting identity. |
| `aede countries` | None            | `--csv`, `--output=<file>`                                                                                                | Summarize artist geographical distributions sourced via MusicBrainz.                 |
| `aede missing`   | `<artist>`      | None                                                                                                                      | Queries MusicBrainz to list missing official studio releases.                        |

### External Metadata & Artwork

| Command      | Arguments         | Key Options                                                                                                                                  | Description                                                                                                |
| :----------- | :---------------- | :------------------------------------------------------------------------------------------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------- |
| `aede fetch` | `[name\|folder…]` | `--summaries`, `--discography`, `--lyrics`, `--covers`, `--portraits`, `--logos`, `--labels`, `--credits`, `--fanart`, `--dry-run`, `--full` | Retrieves attributed metadata, rich credits and derivative assets without modifying audio files.           |
| `aede fetch` | `[name\|folder…]` | `--fanart` with `--no-logo`, `--no-label-logo`, `--no-portrait`, `--no-background`, `--no-banner`, `--no-album-cover`, `--no-cdart`          | Retrieves all useful Fanart.tv image families, minus any explicitly excluded families; 4K backgrounds win. |
| `aede fetch` | `[name\|folder…]` | `--covers --size <250\|500\|1200\|original>`, `--images`                                                                                     | Retrieves missing Cover Art Archive images while leaving every existing local image untouched.             |

Fanart.tv access requires a free key in `AEDE_FANARTTV_KEY`. A complete run can then be tailored without enumerating what should remain enabled:

```sh
# Everything Fanart.tv offers for the local library
aede fetch --fanart

# Everything except portraits and disc artwork
aede fetch --fanart --no-portrait --no-cdart

# Restrict the same selection to one artist or one part of the shelf
aede fetch --fanart --no-banner "Miles Davis"
aede fetch --fanart --no-album-cover ~/Music/Jazz
```

Artist logos, portraits, banners, and backgrounds are written beside the artist's music when there is one shared folder, or under Aède's `assets/` directory otherwise. Label logos live under `assets/labels/<MusicBrainz ID>/`; Fanart.tv album covers and cdART live in each album's `artwork/` directory. Existing files are never overwritten.

Rich credits are fetched only from recording identifiers already present in
the library; no title is guessed:

```sh
aede fetch --credits "Patient Number 9"
aede recording <MusicBrainz-recording-ID>
aede work <MusicBrainz-work-ID>
aede track "Patient Number 9" --json
```

Recording performers and production roles remain distinct from work composers,
lyricists, writers and arrangers. Credited-as names, instruments, qualifiers,
dates, ordering and MusicBrainz relationship identifiers keep their provenance
in `sources.json`. The former `--recordings` option remains a compatibility
alias for `--credits`.

Approximate attachments and exact source identities that conflict with local
tags are reviewed explicitly:

```sh
aede review
aede review --interactive  # compare local and sourced facts, then decide one by one
aede review manson          # narrow the pending list by entity name
aede review --accept=<ID>   # allow this claim into navigation and queries
aede review --reject=<ID>   # retain it as evidence only
aede review --undo=<ID>
```

The decision is persistent and reversible. It is bound to the exact proposed
identifier, never changes the original confidence, and never rewrites an audio
file. `aede doctor` also reports unresolved identities, trusted recordings with
incomplete credits, and contradictions between trusted sources.

The local graph links placements, recordings, works, editions, release groups,
artists and labels in both directions. Guest appearances, compilation
appearances, discography entries and writing or production contributions remain
distinct relations. `aede track`, `recording`, `work`, `album`, `artist` and
`label` expose the relevant paths, while `aede search` also finds recordings,
works and release groups directly.

Entity pages end with copyable `Continue` commands for their related objects.
Search results carry the command that opens each hit, and MusicBrainz
identifiers are preferred wherever they remove title ambiguity. In particular,
`aede release-group <MBID>` leads to every local edition and each edition can
now be opened precisely with `aede album <release-MBID>`.

### Transfer, Export & Derivative Generation

| Command           | Arguments       | Key Options                                                                                                                             | Description                                                                                            |
| :---------------- | :-------------- | :-------------------------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------------------------- |
| `aede copy`       | `<destination>` | `--query`, `--collection`, `--compress <fmt>`, `--quality <q>`, `--extras <mode>`, `--verify`, `--safe-names`, `--dry-run`, `--threads` | Copies audio to external devices, preserving folder layouts and transcoding lossless files on the fly. |
| `aede spectrum`   | `[path]`        | `--size <half\|full>`, `--dry-run`, `--full`, `--threads`                                                                               | Generates $900 \times 470$ or $1800 \times 940$ FFT acoustic spectrogram PNGs via FFmpeg.              |
| `aede playlist`   | `[path]`        | `--simple`, `--artists`, `--dry-run`                                                                                                    | Writes relative `.m3u` playlist files directly into physical album directories.                        |
| `aede collection` | `<name>`        | `--query <expr>`, `--m3u`, `--csv`, `--json`, `--remove`                                                                                | Defines or manages dynamic, self-refreshing smart playlists.                                           |
| `aede export`     | None            | `--csv`, `--tracks`, `--json`, `--output=<file>`                                                                                        | Complete structural vault export in JSON or CSV (album/track level).                                   |

### Forensic Ingestion & Annotations

| Command        | Arguments         | Key Options                                             | Description                                                                  |
| :------------- | :---------------- | :------------------------------------------------------ | :--------------------------------------------------------------------------- |
| `aede import`  | `<path>`          | `--list`, `--pending`, `--forget`, `--source`           | Ingests external FlacCompagnon JSON reports for spectral analysis.           |
| `aede note`    | `<entity> <name>` | `--text <str>`, `--file <path>`, `--append`, `--remove` | Attaches plain-text or Markdown notes to tracks, albums, or artists.         |
| `aede rating`  | `<entity> <name>` | `<1-5>`, `--remove`                                     | Sets a personal star rating ($1\text{--}5$).                                 |
| `aede tag`     | `<entity> <name>` | `<tag_name>`, `--remove`                                | Assigns or removes custom tags.                                              |
| `aede notes`   | None              | `--export`, `--import`, `--output=<file>`               | Backs up or restores user annotations across systems.                        |
| `aede backup`  | `<file.json>`     | None                                                    | Bundles catalog, user annotations, and remote sources into a backup payload. |
| `aede restore` | `<file.json>`     | `--yes`                                                 | Restores vault state from a versioned Aède backup bundle.                    |

---

## 🔍 Relational Query Syntax & Operators

Aède features a unified relational query grammar. Options compose logically via `AND`, `OR`, groupings, range queries, and structural scopes (`album.`, `artist.`, `track.`).

```sh
# Range query with Boolean logic and role matching
aede query "(artist:Ozzy OR artist:Dio) year:1980..1989 album.rating:>=4"

# Querying unplayed favorite tracks
aede query "loved played:0"

# Lossless files larger than 50 MB
aede query "lossless:true size:>50000000"

# Isolating credit contributions
aede query "composer:Rhoads mainartist:Ozzy"

# Traverse the canonical graph and credit relationships
aede query "work:\"War Pigs\" instrument:guitar"
aede query "guest:\"Zakk Wylde\" -compilationartist"
```

Relational fields include explicit tags, exact non-conflicting identities and
source claims accepted through `aede review`. Pending and rejected matches
remain evidence and are not promoted into query results.

### Available Query Fields

| Field                                                                  | Type                    | Description                       | Example Syntax                             |
| :--------------------------------------------------------------------- | :---------------------- | :-------------------------------- | :----------------------------------------- |
| `title`                                                                | Text                    | Track title                       | `title:Interstellar`                       |
| `artist` / `albumartist`                                               | Text                    | Track performer or album artist   | `artist:Coltrane`                          |
| `album`                                                                | Text                    | Album title                       | `album:"Kind of Blue"`                     |
| `recording`                                                            | Text / ID              | Canonical recorded performance   | `recording:"So What"`                      |
| `work`                                                                 | Text / ID              | Composition realised by it       | `work:"War Pigs"`                          |
| `releasegroup`                                                         | Text / ID              | Album identity across editions   | `releasegroup:5c…`                          |
| `genre`                                                                | Text                    | Musical genre                     | `genre:=Jazz`, `genre:Metal`               |
| `label`                                                                | Text                    | Record label imprint              | `label:"Blue Note"`                        |
| `year`                                                                 | Range / Number          | Release year                      | `year:1990..1999`, `year:1994`             |
| `duration`                                                             | Duration                | Length in `mm:ss` or seconds      | `duration:..4:00`, `duration:3:30..5:00`   |
| `size`                                                                 | Bytes                   | File size in bytes                | `size:>50000000`                           |
| `codec` / `format`                                                     | Text                    | Codec name or container extension | `codec:flac`, `format:mp3`                 |
| `bitrate` / `samplerate`                                               | Number                  | Stream parameters                 | `bitrate:>=320k`, `samplerate:96000`       |
| `lossless`                                                             | Boolean                 | Compression state                 | `lossless:true`, `-lossless`               |
| `compilation`                                                          | Boolean                 | Multi-artist compilation flag     | `compilation:true`                         |
| `played`                                                               | Counter                 | Play count                        | `played:0`, `played:>=10`                  |
| `lyrics`                                                               | Text                    | Embedded or `.lrc` sidecar text   | `lyrics:train`                             |
| `comment`                                                              | Text                    | Container ID3/Vorbis comment tag  | `comment:"vinyl rip"`                      |
| **Credits**                                                            |                         |                                   |                                            |
| `composer`, `lyricist`, `producer`, `engineer`, `conductor`, `remixer` | Text                    | Specific liner note credit role   | `composer:Rhoads`, `producer:"Rick Rubin"` |
| `performing`                                                           | Text                    | Anyone audible on the recording   | `performing:"Zakk Wylde"`                  |
| `instrument`                                                           | Text                    | Instrument or credit attribute    | `instrument:guitar`                         |
| `guest` / `compilationartist`                                          | Text                    | Performing participation class    | `guest:"Zakk Wylde"`                       |
| `contributor` / `with`                                                 | Text                    | Non-performing credit / co-performer | `with:"Zakk Wylde"`                      |
| **Annotations**                                                        |                         |                                   |                                            |
| `rating`                                                               | Numeric ($1\text{--}5$) | User star rating                  | `rating:>=4`, `album.rating:5`             |
| `loved`                                                                | Boolean                 | Personal favorite status          | `loved`, `-loved`, `track.loved`           |
| `tag`                                                                  | Text                    | User assigned tag                 | `tag:vinyl`, `album.tag:audiophile`        |
| `note`                                                                 | Text                    | Markdown note content             | `note:remaster`, `artist.note:concert`     |

---

## 🚚 Exporting & Transcoding

When transferring audio to portable devices or external drives, `aede copy` preserves folder layouts while handling non-standard target filesystems cleanly:

1. **Empirical Probe Test:** Writes a temporary, invisible test file to the target filesystem to test forbidden characters (`? * : " < > |`), trailing dots, and DOS reserved names empirically.
2. **Lossless Transcoding Rule:** When `--compress` is active, **only lossless source files** (FLAC, WAV, ALAC) are re-encoded. Existing lossy files (MP3, AAC, Opus) are copied untouched to prevent generation loss.
3. **Threading Optimization:** Transcoding jobs run in parallel across all CPU cores. Plain uncompressed file transfers queue sequentially to prevent disk head thrashing on mechanical drives or SD cards.

```sh
# Copy loved tracks to a phone SD card, encoding FLACs to Opus @ 128k
aede copy /Volumes/Phone --query "loved" --compress opus --quality 128k

# Copy a saved collection with CRC-32 read-back verification
aede copy /Volumes/Player --collection wishlist --verify
```

---

## 💾 Vault State & Storage Footprint

All metadata and state persist in a unified directory configured via `$AEDE_HOME` or defaulting to `$XDG_DATA_HOME/aede` (`~/.local/share/aede/`).

```
~/.local/share/aede/
├── catalog.json      # Derived index, file hashes, integrity verdicts
├── user.json         # Irreplaceable annotations, collections, merges, roots
└── sources.json      # Attributed source data and reversible review decisions
```

### Storage Benchmarks

| Tracks      | `catalog.json` Size | Save Time | Load Time | Peak RAM |
| :---------- | :------------------ | :-------- | :-------- | :------- |
| **10,000**  | 12.4 MB             | 0.79 s    | 0.41 s    | 181 MB   |
| **50,000**  | 62.5 MB             | 3.88 s    | 2.17 s    | 897 MB   |
| **200,000** | 252.0 MB            | 16.37 s   | 13.42 s   | 3 586 MB |

---

## 📖 Documentation

Complete guides to Aède's features and architecture:

- [Library Fundamentals](docs/library.md) — Overview of the catalog structure
- [Querying](docs/querying.md) — Complete query language and syntax
- [Browsing](docs/browsing.md) — Interactive navigation and exploration
- [Commands](docs/commands.md) — Full command reference
- [Copying & Transfer](docs/copying.md) — Managing exports and transcoding
- [Playlists](docs/playlists.md) — Playlist generation and management
- [Annotations](docs/annotating.md) — User tags, ratings, and notes
- [Audio Format Support](docs/formats.md) — Container and codec details
- [Integrity Checking](docs/integrity.md) — Bit rot detection and verification
- [Spectrogram Analysis](docs/spectrograms.md) — Visual spectral analysis
- [Imported Analyses](docs/imported-analyses.md) — Integration of external metrics
- [Source Attribution](docs/sources.md) — MusicBrainz and external metadata handling
- [Licensing & Copyright](docs/licensing.md) — Licensing and usage rights

### Design & Architecture

- [Architecture Overview](docs/design/architecture.md) — System design principles
- [Attribution & Provenance](docs/design/attribution.md) — Data source tracking
- [Canonical Music Graph](docs/design/music-graph.md) — Canonical entities and relationship assertions
- [Querying Design](docs/design/querying.md) — Query engine architecture
- [Lyrics Handling](docs/design/lyrics.md) — Lyrics ingestion and formatting
- [Playback & Gapless Audio](docs/design/playback.md) — Precise sample-accurate playback
- [Release Identification](docs/design/identification.md) — Matching releases to recordings
- [Annotations Design](docs/design/annotations.md) — User metadata model
- [Library Paths](docs/design/paths.md) — Directory structure and organization
- [Discogs Integration](docs/design/discogs.md) — Discogs source handling
- [Interoperability](docs/design/interoperability.md) — Integration with other tools
- [Plugin Architecture](docs/design/plugins.md) — Extension points and customization
- [Strategic Conclusions](docs/design/conclusions.md) — Design decisions and trade-offs
- [Roadmap & Milestones](docs/design/roadmap.md) — Future development plans

### Development & Engineering

- [Current State](docs/coding/current-state.md) — Project status and active work
- [Engineering Rules](docs/coding/engineering-rules.md) — Development guidelines and standards

---

## ⚖️ License & Archival Ethos

Aède is open-source software released under the **Mozilla Public License v2.0**.

Designed for collectors who view digital music not as disposable streams, but as an irreplaceable historical record requiring meticulous care, clear provenance, and persistent ownership.
