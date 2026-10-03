# credits — Measure MusicBrainz recording/work and edition credit coverage

credits audits MusicBrainz recording/work and exact-edition credit coverage offline. Without a selection, it groups counts by album; with an album title, release ID or folder, it expands representative recordings and filenames. Recording counts refer to canonical recordings, not every duplicate file placement. Edition counts refer to local releases: editions sharing the same recordings remain separate.

credited means usable sourced credits exist; empty means a completed lookup returned none; waiting means a lookup is still needed; untrusted means evidence is held but not eligible; unidentified means no usable recording identity. These statuses prevent “no credits” from being confused with “not fetched”. Recording-level and work-level credits are counted separately.

Edition coverage uses the same statuses, but requires a non-empty local MusicBrainz release ID and a completed, trusted answer for that exact edition. A release-group identity or a recording lookup cannot complete the edition lookup. Manual edition credits are counted separately: they do not claim that MusicBrainz was queried.

If a recording or edition is waiting, copy the folder-scoped fetch command printed by the report. --full belongs to fetch when you deliberately repeat a completed answer, not to credits. --json preserves the recording fields and adds edition coverage for tools; no network call occurs.

## Syntax and arguments

```text
aede credits [album title|MusicBrainz ID|folder] [--json]
```

Optional album title, MusicBrainz release ID or existing folder.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede credits
aede credits "Kind of Blue"
aede credits "$HOME/Music/Jazz" --json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[fetch](fetch.md), [credit](credit.md), [recording](recording.md), [work](work.md).

Detailed existing guide: [commands.md](../commands.md).
