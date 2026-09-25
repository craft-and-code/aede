# Building the Library: Archival Management & Vault Structure

## Tagging: Metadata Integrity with MusicBrainz Picard

Aède maintains a strict boundary: **it never writes to your master audio files**. Tags, file names, and folder hierarchies on your hard drive remain completely untouched. This immutability guarantees archival integrity, but it means high-quality initial metadata must be provided by dedicated tagging tools.

[MusicBrainz Picard](https://picard.musicbrainz.org/) is the recommended companion for this task. The two tools work in harmony rather than competition: Picard inspects your audio, queries the MusicBrainz database, and writes standardized tags (`MUSICBRAINZ_*` tags); Aède reads these tags, indexes the structure, and builds your searchable catalog.

When a library is pre-tagged with Picard, Aède seamlessly binds entity IDs to global MusicBrainz relationships and credits without guessing. Metadata conflicts become exceptionally rare because your local tags and the global registry share a common origin. If you choose not to use Picard, Aède will still parse standard ID3, Vorbis, or MP4 tags gracefully, and `aede doctor` will point out any missing or incomplete metadata fields.

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

Exclusion updates take effect immediately by running a target rescan in the background:

- Adding or removing an exclusion automatically re-indexes the relevant paths.
- Pass `--no-scan` to defer indexing when batching multiple exclusion changes across slow storage.
- `aede reset` leaves root configuration intact until explicitly cleared, preserving your administrative preferences.

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

Compilations—releases featuring tracks by multiple distinct artists without a single release-level artist—are flagged automatically during scanning. They are excluded from individual artist discographies to prevent catalog clutter.

You can query compilations directly using dedicated flags:

```
aede albums --compilations       # list only multi-artist compilations
aede albums --no-compilations    # list only single-artist releases
```

Passing both flags simultaneously is rejected as a logical contradiction.

## Interpreting the Scan Report

Every `aede scan` concludes with a comprehensive diagnostic breakdown:

| Metric                        | Description                                                                                           |
| :---------------------------- | :---------------------------------------------------------------------------------------------------- |
| **Files found**               | Total audio files discovered during directory traversal (duplicates removed).                         |
| **Read from disk**            | Files whose metadata tags were parsed (new or modified since last scan).                              |
| **Reused from previous scan** | Files unchanged in path, size, and modification timestamp; metadata loaded instantly from catalog.    |
| **Gone since last scan**      | Files previously indexed but no longer present on disk; safely purged from catalog.                   |
| **Analyses imported**         | External [FlacCompagnon reports](imported-analyses.md#what-another-tool-found) detected and ingested. |
| **Analyses now attached**     | Previously pending external analyses successfully linked to newly scanned files.                      |
| **Elapsed**                   | Total wall-clock duration for storage walk and metadata ingestion.                                    |

The total `Files found` equals the sum of `Read from disk` and `Reused from previous scan`. Files that encounter read errors are flagged below the summary table and skipped without aborting the scan.

## Vault Location, Storage Footprint & Scaling

Catalog metadata and system state are stored in a unified configuration directory:

```
aede --data=/volume1/aede stats     # override catalog path for a single command
export AEDE_HOME=/volume1/aede      # globally relocate catalog storage
```

`aede stats` provides clear visibility into catalog footprint and storage location:

```
This catalog

  Kept in       /Users/kcell/.local/share/aede
  Weighs        11.2 MB
  Last scanned  3 days ago
  aede backup writes all four stores to one file; AEDE_HOME moves them
```

If `AEDE_HOME` is unset, Aède falls back to `$XDG_DATA_HOME/aede` or `~/.local/share/aede`.

The catalog uses an in-memory JSON document model for queries. The following synthetic-library measurements (12 tracks per album) predate the separation of `conclusions.json`; current sizes and timings have not yet been remeasured:

| Tracks      | `catalog.json` Size | Save Time | Load Time | Memory Usage (Peak) |
| :---------- | :------------------ | :-------- | :-------- | :------------------ |
| **10,000**  | 12.4 MB             | 0.79 s    | 0.41 s    | 181 MB              |
| **50,000**  | 62.5 MB             | 3.88 s    | 2.17 s    | 897 MB              |
| **200,000** | 252.0 MB            | 16.37 s   | 13.42 s   | 3,586 MB            |

Those historical measurements show that libraries up to 50,000 tracks loaded in approximately two seconds with under 1 GB RAM usage. For archives exceeding 100,000 tracks, RAM usage increases proportionally. Advanced architectural options for massive collections are detailed in [Architecture](design/architecture.md#when-this-becomes-a-database).

## Safeguarding Your Data: Backups & Disaster Recovery

Aède consolidates complete system state into a portable, versioned backup file:

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
→ /Users/kcell/aede-2026-09-03.json (9.7 MB)
```

Each store maintains its own format version within the backup payload. When restoring, Aède inspects store versions independently. If a backup lacks a specific store (e.g., an export created before remote metadata was fetched), existing local stores of that type remain untouched rather than being overwritten or deleted.

Before performing a restore, Aède summarizes the operation and prompts for confirmation:

```
Restore

  made 3 days ago by Aède 0.2.0
  into /Users/kcell/.local/share/aede
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
  /Users/kcell/Music         746   2 days    112.4 GB
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
  a scan rebuilds the catalog; the watched folders and the integrity
  verdicts are lost and have to be redone
  the imported analyses go too, and have to be imported again
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
