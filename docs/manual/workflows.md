# Daily workflows

Run these examples with titles/folders that exist in your catalog. Choose one data directory for every cooperating command; add `--data` consistently or set `AEDE_HOME` in the process environment.

## Add newly acquired files

Place the files with your chosen folder layout, then run `aede scan` if they are inside a watched root. To add another root, run `aede scan "/path/to/second-library"`. Follow with `aede doctor`; it reports issues without retagging or deleting duplicates. Aède does not reorganize originals from tags.

## Connect the people behind an album

```sh
aede fetch --credits "/path/to/album"
aede credits "Album title"
aede album "Album title"
aede relations "Contributor name"
```

Recording credits describe performers/production, work credits describe creation, and release credits describe one exact edition. Copy an artist/recording/work ID from Continue. Resolve uncertain identities with review; correct an exact sourced credit with credit, preserving its source evidence. Discography comparisons require `fetch --discography` before `missing`.

## Save a portable selection

```sh
aede collection Road --query "loved"
aede copy "/Volumes/Player" --collection Road --dry-run
aede copy "/Volumes/Player" --collection Road --verify
```

Inspect the preview before a large copy. For a smaller player format, add `--compress mp3 --quality V0`; FFmpeg is required and only lossless sources are re-encoded. The destination must be separate from the catalog roots. A collection is dynamic: new matching tracks join on the next evaluation.

## Reuse FlacCompagnon measurements

```sh
aede import "/path/to/artist-report.json"
aede scan "/path/to/music"
aede import --list
aede track "Track title"
```

Alternatively `aede analyze "/path/to/album" --show-results` measures in process. Reports can be saved with `--json-layout album|artist`. Current source LUFS/true peak can inform playback normalization, while stale measurements are excluded. Analysis is distinct from container checksum verification: run check for its supported CRCs and inspect its verdicts.

## Keep the catalog API running

```sh
aede scan "/path/to/music" --data "/path/to/aede-data"
aede serve --data "/path/to/aede-data"
```

Use a second terminal for catalog queries and same-data CLI writes. On Unix those writes delegate to the server. Closing the writing CLI does not cancel scan/fetch; use the printed ID with cancel. The server is local-only and exposes no audio playback route. Read [serve](../cli/serve.md) before service/NAS experiments.

## Before a meaningful change

Create `aede backup before-change.aede`, and preserve original audio separately. `notes --export` transfers personal annotations; `rules --export` transfers reproducible decisions; `export --graph` provides an attributed graph for inspection. Read their respective scope before treating one as a recovery file.
