# stats — Show the library's size, quality, and completeness

stats describes the whole catalog: tracks, albums, artists, duration, space and quality categories. Lossless, hi-res and lossy summaries describe file characteristics; they do not prove the recording’s audible quality or original source.

Use this after the first scan to check that the expected folders were read. Pagination applies separately to each subsidiary list (formats, quality, sample rates, decades, countries, roles and rankings), in both human and JSON output. It never changes library totals or completeness ratios. --json returns a structured report for tools, but stats has no --output export option: shell redirection can save its standard output.

The data-folder weight includes `catalog.json`, `conclusions.json`, `user.json` and `sources.json`. Aggregate durations and sizes saturate at the largest supported integer if malformed or imported measures would overflow it; ordinary library values are summed exactly.

No metadata or music is changed. When a count surprises you, inspect roots and doctor, then scan again. A missing catalog means scanning is needed, not that stats should be pointed at the music with --data.

## Syntax and arguments

```text
aede stats
```

No positional arguments; statistics describe the selected data directory.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--limit N` | Show at most N rows per subsidiary list; use a positive whole number. Default: 10. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row from the requested offset. Refused with --limit. |
| `--json / -j` | Write the structured statistics, with the same list pagination. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede stats
aede stats --json
aede stats --json > stats.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md), [doctor](doctor.md).

Detailed existing guide: [library.md](../library.md).
