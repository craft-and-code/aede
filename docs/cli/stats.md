# stats — Show the library's size, quality, and completeness

stats describes the whole catalog: tracks, albums, artists, duration, space and quality categories. Lossless, hi-res and lossy summaries describe file characteristics; they do not prove the recording’s audible quality or original source.

Use this after the first scan to check that the expected folders were read. Pagination limits long subsidiary lists rather than selecting a different library. --json returns a structured report for tools, but stats has no --output export option: shell redirection can save its standard output.

No metadata or music is changed. When a count surprises you, inspect roots and doctor, then scan again. A missing catalog means scanning is needed, not that stats should be pointed at the music with --data.

## Syntax and arguments

```text
aede stats
```

No positional arguments; statistics describe the selected data directory.

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
aede stats
aede stats --json
aede stats --json > stats.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md), [doctor](doctor.md).

Detailed existing guide: [library.md](../library.md).
