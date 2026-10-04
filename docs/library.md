# Building the Library: Archival Management & Vault Structure

## Tagging: Metadata Integrity with MusicBrainz Picard

Aède maintains a strict boundary: **it never writes to your master audio files**. Tags, file names, and folder hierarchies on your hard drive remain completely untouched. This immutability guarantees archival integrity, but it means high-quality initial metadata must be provided by dedicated tagging tools.

[MusicBrainz Picard](https://picard.musicbrainz.org/) is the recommended companion for this task. Aède was developed around a library prepared with Picard, whose MusicBrainz identifiers provide a useful basis for identities and credits. Picard queries the MusicBrainz database and writes standardized tags (`MUSICBRAINZ_*` tags); Aède reads these tags and builds your searchable catalog. Picard is a recommendation, not a prerequisite: another tag editor that suits your collection is equally welcome.

When a library is pre-tagged with Picard, Aède reads its MusicBrainz identifiers to establish local entity identities. An explicit `aede fetch --credits` can then obtain attributed MusicBrainz relationships and credits; scanning alone never makes that network request. Shared identifiers reduce identity ambiguity but do not guarantee agreement between local tags and external claims. If you choose not to use Picard, Aède still reads standard ID3, Vorbis or MP4 tags, and `aede doctor` points out missing or incomplete metadata fields.

## Aède and beets

[beets](https://docs.beets.io/en/stable/guides/main.html) provides music importing, tagging, organization and queries. Aède catalogs an existing collection and connects its recordings, editions, works and credits while keeping original audio and tags unchanged.

| Task | beets | Aède |
| --- | --- | --- |
| Prepare tags | Its importer can apply identified metadata to files. | Reads existing tags; tag preparation belongs to a tool such as Picard or beets. |
| Organize files | Import can copy files into an organized library or move them when configured. | Keeps original names and folders; an explicit `copy` creates a separate destination. |
| Protect originals | Supports a read-only import configuration. | Preserving original audio and tags is a permanent rule for every command. |
| Add context | Metadata matching and plugins extend the library. | External claims, analysis results and personal annotations are stored separately from file tags, with their origins retained. |

The [beets configuration reference](https://docs.beets.io/en/stable/reference/config.html#importer-options) documents tag writing and copying enabled by default, with moving disabled by default. Its [read-only import mode](https://docs.beets.io/en/stable/guides/main.html#basic-configuration) disables copying and tag writing; beets does not inherently require modifying originals. The distinction is that Aède's preservation rule does not depend on an import setting. Its explicit downloads and analyses can create separate sidecar files, and `copy` can convert destination files, while source audio and tags remain intact.

The tools can be complementary: prepare tags with Picard or beets when desired, review the result, then scan the collection with Aède. Rescan after changing tags so the catalog reflects the new local metadata.

## Resolving Identity: When One Artist Appears Twice

When files are processed through Picard, they carry unique `MUSICBRAINZ_ARTISTID` tags. During a catalog scan, Aède automatically merges variant spellings—such as `Ozzy Osbourne` and `O. Osbourne`—into a single artist entry without heuristic guesswork. The artist’s detailed page explicitly lists all absorbed aliases.

For untagged or non-MusicBrainz files, Aède strictly avoids risky string-matching heuristics; guessing on partial names risks disastrously merging unrelated artists (such as Angus Young and Neil Young). Instead, `aede doctor` highlights suspicious duplicate pairs for review, allowing you to manually unify entities with `aede merge`. Neither operation ever modifies an audio file on disk.

## Folder Exclusions: Guarding the Vault Boundaries

An audio directory often contains material that does not belong in a music CDthèque—such as audiobooks, podcasts, staging folders (`_incoming`), or DAW sample packs. Rather than forcing you to alter your file system layout to fit the application, Aède lets you exclude specific paths:

```
aede roots --exclude ~/Music/Audiobooks     # permanently ignore this directory
aede roots                                  # display all watched and excluded roots
aede roots --exclude ~/Music/Audiobooks --remove
```

Exclusion rules are stored directly inside the catalog alongside watched roots. This design choice ensures that exclusions persist across full library rescans. **A scan must never destroy data it cannot recompute**, and user-defined exclusions are explicit curation rules.

Path matching uses canonical resolution, meaning symbolic links pointing to excluded folders are properly honored.

Exclusion updates take effect by automatically rescanning the watched folders before the command finishes:

- Adding or removing an exclusion automatically re-indexes the relevant paths.
- Pass `--no-scan` to defer indexing when batching multiple exclusion changes across slow storage.
- `aede reset` removes the catalog, including watched roots and exclusions; independent conclusions, personal data and source stores remain intact.

## Disc Anatomy: Box Sets & Multi-Disc Releases

Box sets and special editions are frequently organized into multi-disc subfolders:

```
Nobuo Uematsu/1997 FINAL FANTASY VII [FLAC]/Disc 1/
Nobuo Uematsu/1997 FINAL FANTASY VII [FLAC]/Disc 2/
```

While top-level album folders distinguish separate masterings or editions, subdirectories such as `Disc 1`, `CD2`, or `Disque 3` represent physical subdivisions of a single release. Aède automatically folds these subdirectories into the parent album entry.

Track listings represent disc numbering using standard notation (`1-01`, `2-07`). Multi-disc notation is displayed only on multi-volume albums to keep single-disc tracklists clean. Disc numbers are extracted from `DISCNUMBER` tags when present, or inferred from folder structures when tags are missing.

Album summaries report the actual number of physical discs present:

```
  4 discs · 85 tracks · 4:34:11 · 1.5 GB
```

If a four-disc set is missing its final disc on disk, Aède accurately reports `3 discs`, reflecting the physical state of your archive rather than theoretical tag metadata.

## Managing Compilations

Scanning marks a release as a compilation when a `COMPILATION` tag is `1`, `true` or `yes`, or when its album artist is a recognized placeholder such as `Various Artists`, `VA` or `Artistes divers`. These releases are excluded from individual artist discographies to prevent catalog clutter.

Different track artists alone do not establish a compilation. Without an album artist or a compilation tag, the files can form separate local releases for their track artists. Tag a multi-artist compilation consistently before scanning it.

You can query compilations directly using dedicated flags:

```
aede albums --compilations       # list releases marked as compilations
aede albums --no-compilations    # list other releases
```

Passing both flags simultaneously is rejected as a logical contradiction.

## Interpreting the Scan Report

Every `aede scan` concludes with a comprehensive diagnostic breakdown:

| Metric                        | Description                                                                                           |
| :---------------------------- | :---------------------------------------------------------------------------------------------------- |
| **Files found**               | Total audio files discovered during directory traversal (duplicates removed).                         |
| **Read from disk**            | Files selected for a fresh metadata read, including attempts that later report an error.              |
| **Reused from previous scan** | Files unchanged in path, size, and modification timestamp; metadata loaded instantly from catalog.    |
| **Gone since last scan**      | Files previously indexed and confirmed absent from accessible paths; removed from the index.                   |
| **Analyses imported**         | External [FlacCompagnon reports](imported-analyses.md#what-another-tool-found) detected and ingested. |
| **Analyses now attached**     | Previously pending external analyses successfully linked to newly scanned files.                      |
| **Elapsed**                   | Total wall-clock duration for storage walk and metadata ingestion.                                    |

`Files found` counts discovered audio paths; retained inaccessible entries are reported separately. `Read from disk` and `Reused from previous scan` describe metadata work on discovered files. Read errors are reported below the summary. Previous entries from inaccessible paths are retained, including during a full scan, and counted separately as preserved; an unreadable folder is not treated as empty.

## Vault Location, Storage Footprint & Scaling

Catalog metadata and system state are stored in a unified configuration directory:

```
aede --data=/volume1/aede stats     # override catalog path for a single command
export AEDE_HOME=/volume1/aede      # globally relocate catalog storage
```

`aede stats` provides clear visibility into catalog footprint and storage location:

```
This catalog

  Kept in       /Users/alice/.local/share/aede
  Weighs        11.2 MB
  Last scanned  3 days ago
  aede backup writes all four stores to one file; AEDE_HOME moves them
```

If `AEDE_HOME` is unset, Aède falls back to `$XDG_DATA_HOME/aede` or `~/.local/share/aede`.

The catalog uses an in-memory JSON document model for queries. The following synthetic-library measurements (12 tracks per album) predate the separation of `conclusions.json`; these historical figures should not be used as current deployment budgets. A newer synthetic baseline is recorded in [M2 storage measurements](coding/m2-storage-benchmark.md):

| Tracks      | `catalog.json` Size | Save Time | Load Time | Memory Usage (Peak) |
| :---------- | :------------------ | :-------- | :-------- | :------------------ |
| **10,000**  | 12.4 MB             | 0.79 s    | 0.41 s    | 181 MB              |
| **50,000**  | 62.5 MB             | 3.88 s    | 2.17 s    | 897 MB              |
| **200,000** | 252.0 MB            | 16.37 s   | 13.42 s   | 3,586 MB            |

Those historical measurements show that libraries up to 50,000 tracks loaded in approximately two seconds with under 1 GB RAM usage. For archives exceeding 100,000 tracks, RAM usage increases proportionally. Advanced architectural options for massive collections are detailed in [Architecture](design/architecture.md#when-this-becomes-a-database).

## Safeguarding Your Data: Backups & Disaster Recovery

Aède consolidates its JSON data stores into a portable, versioned backup file. The bundle does not contain original music or derivative image/lyric files, which need separate backups:

```
aede backup ~/aede-2026-09-03.json    # export catalog state and annotations
aede restore ~/aede-2026-09-03.json   # restore system state from backup
```

The backup bundle preserves four stores:

1. **Catalog Store (`catalog.json`):** Scanned metadata, derived graph, watched folders and exclusions.
2. **Conclusions Store (`conclusions.json`):** Integrity verdicts, fingerprints and imported analyses.
3. **User Store (`user.json`):** Irreplaceable user data—notes, ratings, play counts, custom collections, manual merges, and ignored items.
4. **Source Store (`sources.json`):** Remote metadata fetched from external services (e.g., MusicBrainz).

```
Backup

  catalog            20 148 tracks, 1 604 albums
  conclusions        18 412 file results, 236 analyses
  what you said      312 annotations, 4 collections, 9 records set aside
  what sources said  1 841 records
→ /Users/alice/aede-2026-09-03.json (9.7 MB)
```

Each store maintains its own format version within the backup payload. When restoring, Aède inspects store versions independently. If a backup lacks a specific store (e.g., an export created before remote metadata was fetched), existing local stores of that type remain untouched rather than being overwritten or deleted.

Before performing a restore, Aède summarizes the operation and prompts for confirmation:

```
Restore

  made 3 days ago by Aède 0.3.0
  into /Users/alice/.local/share/aede
  catalog            20 148 tracks, 1 604 albums — replaces what is there
  what you said      312 annotations — replaces what is there
  what sources said  not in this backup — left as it is
```

If a restored watched directory is unmounted or unreachable, Aède issues an explicit warning prior to restoration to prevent accidental catalog truncation during subsequent scans. In non-interactive environments, pass `--yes` to acknowledge prompts.

## Catalog Management & Resetting State

`aede roots` summarizes watched storage directories:

```
Watched folders

  Folder                 Tracks  Duration       Size
  ─────────────────────  ──────  ────────  ─────────
  /Volumes/Music/FLAC     18 402   52 days     4.1 TB
  /Users/alice/Music         746   2 days    112.4 GB
  (no longer watched)         92   6 h        8.1 GB
```

To clear catalog indexes without affecting physical audio files, use `aede reset`:

```
$ aede reset

About to remove the catalog

  Tracks                  20 148
  Watched folders              2
  Integrity verdicts      20 148
  Imported analyses          312
  File                    9.4 MB
  a scan rebuilds the catalog; watched folders must be named again
  integrity verdicts, fingerprints and imported analyses are kept
  Type "yes" to confirm:
```

Passing `--yes` bypasses interactive confirmation for automated scripts.

## Testing with Synthetic Datasets

You can test library features using the bundled test suite generator:

```
tools/demo-library.sh /tmp/demo-music   # generates synthetic test audio (requires ffmpeg)
aede scan /tmp/demo-music
aede doctor
```

The generated test environment includes intentional metadata issues—such as missing tags, duplicate files, missing tracks, and mixed codecs—providing a complete sandbox to evaluate `aede doctor` and catalog management workflows.

## Previewing changes before publication

```sh
aede scan --dry-run
aede scan /Volumes/Archive --dry-run --json
```

The preview reports added, changed, removed and unreadable paths without publishing catalog, roots, conclusions or personal data. It may read audio metadata and local analysis reports to prepare the proposed graph. JSON emits all change paths plus counts, without terminal progress mixed into standard output. Precise timestamps distinguish same-size edits within one second when the filesystem provides that resolution; legacy catalog rows lacking subsecond precision are reread once.

Imported acoustic reports keep the source tool’s recorded identity and whole-second timestamp precision. The new scan precision protects metadata reuse and native integrity/fingerprint attachment; it cannot establish subsecond freshness for an external report that never recorded it.
