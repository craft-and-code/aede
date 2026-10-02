# genre — Show albums and artists carrying one genre

genre opens one locally tagged genre and its associated albums/artists. It does not classify audio automatically. Names are matched against the catalog; use genres to discover the spellings present.

Track selections underlying CSV/JSON/M3U use the shown genre scope. Pagination affects export, so --all is useful for a complete genre playlist. This read-only view cannot assign a genre to music; personal labels belong to tag, and original metadata remains under your separate tagging tool’s control.

## Syntax and arguments

```text
aede genre <name>
```

One catalogued genre name.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--csv` | Write a CSV table of this command’s result, suitable for a spreadsheet; mutually exclusive with other output formats. |
| `--m3u` | Write an M3U playlist containing the selected local tracks. It contains paths, not copies of the audio. |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--separator ";" / --separator tab` | With --csv only: use comma by default, a one-character separator such as ;, or tab for tab-separated text. Quote ; in a shell. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede genre Jazz
aede genre Jazz --all --m3u --output jazz.m3u8
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[genres](genres.md), [query](query.md).

Detailed existing guide: [browsing.md](../browsing.md).
