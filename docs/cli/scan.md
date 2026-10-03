# scan — Read watched music folders into the local catalog

Scan reads supported audio files recursively, derives the graph from their tags and saves a catalog in the data directory. The first scan needs at least one readable music folder. Subsequent scans without an argument reuse watched folders. A folder supplied later is added, not substituted.

The normal scan reuses unchanged files. --full rereads metadata but keeps the watched roots. --replace keeps only the folders named in this invocation, so give all folders you intend to retain. Excluded roots remain excluded. An unplugged watched drive is reported and its previous entries are preserved while other roots are scanned. Newly supplied unavailable folders are refused; deliberately removing a root removes its entries.

The summary reports scanned files, albums, artists and parse issues. A successful scan is an index, not an audio integrity verdict. Imported FlacCompagnon reports beside the scanned folders are attached to matching files; changed files invalidate stale conclusions. Audio and tags are never rewritten.

## Syntax and arguments

```text
aede scan [folder…]
```

Zero or more existing music-folder paths. Quote a path containing spaces.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--dry-run` | Show added, changed, removed and unreadable paths without saving catalog, roots or personal data. Metadata may be read to prepare the result. |
| `--json / -j` | Produce a structured change report for either a preview or a published scan. |
| `--full` | Bypass unchanged-file metadata reuse and reread tags; keep watched roots and existing separate stores. |
| `--threads N` | Number of worker threads. A positive integer fixes the count; 0 selects an automatic count. Plain copying defaults to one worker. |
| `--replace` | Keep only the watched folders explicitly supplied in this scan; existing exclusions remain. |
| `--follow-symlinks` | During scan, follow symbolic links to files/folders. Without this flag, links are not followed. |
| `--include-hidden` | Include files and folders whose names are hidden during scan; they are skipped by default. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede scan "$HOME/Music"
aede scan "/Volumes/Archive musicale"
aede scan
aede scan --full
aede scan --dry-run --json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[roots](roots.md), [check](check.md), [analyze](analyze.md).

Detailed existing guide: [library.md](../library.md).

An inaccessible subtree retains its previously indexed entries, including with --full, and reports the access failure. Precise modification timestamps are used when supplied by the filesystem; older catalogs lacking the precision are reread once. JSON contains dry_run, roots, added, changed, removed, unreadable and counts; preserved counts retained entries from inaccessible paths.

Only regular files are opened, so a special file with an audio extension cannot block the scan. Paths that cannot be represented as UTF-8 are reported and skipped; non-UTF-8 roots are refused. With --follow-symlinks, aliases are selected in deterministic native path order. A file changed or replaced during metadata reading is reported as temporarily unreadable and its cached entry is preserved.
