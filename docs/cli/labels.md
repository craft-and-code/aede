# labels — List record labels in the catalog

labels groups the local record-label/imprint metadata and associated album/track/duration totals. Label names from tags and trusted source identities retain their origin; two similarly named labels are not silently declared identical. Sort by name/title, albums, tracks, duration/length or size.

This plural command takes no positional entity name. Use the singular/detail command to open one item rather than appending a name to the list. The displayed order is deterministic. Where pagination is offered, --offset counts skipped rows from zero, --limit must be positive and --all cannot be combined with --limit. CSV/JSON exports contain the filtered, paged result shown, not automatically the whole library. Add --all when supported for a complete export. The listing is read-only.

## Syntax and arguments

```text
aede labels
```

No positional arguments. Use the filters listed below.

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
| `--sort ORDER` | Choose a supported column; append - for descending order, for example duration-. Supported values for this command are listed below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede labels
aede labels --sort albums-
aede labels --all --csv --output labels.csv
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[label](label.md), [query](query.md).

Detailed existing guide: [browsing.md](../browsing.md).
