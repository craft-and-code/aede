# favourites — List everything marked as a favourite

favourites lists entities you explicitly marked through love: artists, albums, tracks and other supported annotated kinds. It is an entity list, not an inherited playlist of every track under those favorites. favorites is an identical spelling alias.

Use query "loved" for the inherited track selection or query "track.loved" for directly favored tracks. Export this list with CSV/JSON and --output; M3U is deliberately unavailable because entity rows are not all playable tracks. Pagination applies before export. Remove a flag with love KIND NAME --remove, not this read-only command.

Alias: `aede favorites`. Options and behavior are identical.

## Syntax and arguments

```text
aede favourites
```

No positional arguments.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--csv` | Write a CSV table of this command’s result, suitable for a spreadsheet; mutually exclusive with other output formats. |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--separator ";" / --separator tab` | With --csv only: use comma by default, a one-character separator such as ;, or tab for tab-separated text. Quote ; in a shell. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede favourites
aede favorites --all --csv --output favourites.csv
aede query "loved" --m3u
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[love](love.md), [query](query.md).

Detailed existing guide: [annotating.md](../annotating.md).
