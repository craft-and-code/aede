# Aède — Archival Music Library Manager

> _A digital sanctuary for serious music collectors, archivists, and audio curators._

> [!TIP]
> An _aède_ (Greek ἀοιδός, _aoidos_) was the poet-singer of archaic Greece: he held the whole repertoire in memory and performed it. Keeping and playing, in one word — which is exactly what this program is for.

**Aède** is a high-precision, read-only local music library manager and cataloging system written in Rust. Designed with an uncompromising commitment to archival integrity, Aède treats your master music collection as a sanctuary: it reads metadata, verifies audio container integrity, indexes complex credit graphs, and generates derivative assets—**without ever writing a single byte back into your original audio files**.

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
| `aede doctor` | None         | None                                        | Run a health check: missing metadata, duplicates, bit rot, broken links.                    |
| `aede stats`  | None         | None                                        | Displays catalog metrics, audio quality distribution, and credit roles.                     |
| `aede reset`  | None         | `--yes`                                     | Wipes indexed catalog data while preserving root configurations.                            |

### Query, Search & Catalog Browsing

| Command          | Arguments      | Key Options                                                                                                               | Description                                                          |
| :--------------- | :------------- | :------------------------------------------------------------------------------------------------------------------------ | :------------------------------------------------------------------- |
| `aede query`     | `<expression>` | `--m3u`, `--csv`, `--json`                                                                                                | Evaluates a relational search expression across the catalog graph.   |
| `aede search`    | `<term>`       | `--comments`, `--notes`, `--lyrics`                                                                                       | Free-text search across titles, artists, albums, or prose metadata.  |
| `aede albums`    | None           | `--artist`, `--genre`, `--year`, `--compilations`, `--no-compilations`, `--limit`, `--offset`, `--all`, `--csv`, `--json` | List and filter album records with pagination support.               |
| `aede artists`   | None           | `--role <role>`, `--country <code>`, `--limit`, `--offset`, `--all`, `--csv`, `--json`                                    | List artists, filter by credit role, or map by geographic origin.    |
| `aede genres`    | `[name]`       | `--m3u`, `--csv`, `--json`                                                                                                | Browse music genres or export tracks matching a specific genre.      |
| `aede labels`    | `[name]`       | `--m3u`, `--csv`, `--json`                                                                                                | Survey record imprints and catalog releases.                         |
| `aede countries` | None           | `--csv`, `--output=<file>`                                                                                                | Summarize artist geographical distributions sourced via MusicBrainz. |
| `aede missing`   | `<artist>`     | None                                                                                                                      | Queries MusicBrainz to list missing official studio releases.        |

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
```

### Available Query Fields

| Field                                                                  | Type                    | Description                       | Example Syntax                             |
| :--------------------------------------------------------------------- | :---------------------- | :-------------------------------- | :----------------------------------------- |
| `title`                                                                | Text                    | Track title                       | `title:Interstellar`                       |
| `artist` / `albumartist`                                               | Text                    | Track performer or album artist   | `artist:Coltrane`                          |
| `album`                                                                | Text                    | Album title                       | `album:"Kind of Blue"`                     |
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
└── sources.json      # Cached MusicBrainz relationship data
```

### Storage Benchmarks

| Tracks      | `catalog.json` Size | Save Time | Load Time | Peak RAM |
| :---------- | :------------------ | :-------- | :-------- | :------- |
| **10,000**  | 12.4 MB             | 0.79 s    | 0.41 s    | 181 MB   |
| **50,000**  | 62.5 MB             | 3.88 s    | 2.17 s    | 897 MB   |
| **200,000** | 252.0 MB            | 16.37 s   | 13.42 s   | 3 586 MB |

---

## ⚖️ License & Archival Ethos

Aède is open-source software released under the **MIT License**.

Designed for collectors who view digital music not as disposable streams, but as an irreplaceable historical record requiring meticulous care, clear provenance, and persistent ownership.
