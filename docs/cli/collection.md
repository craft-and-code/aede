# collection — Save, run, or remove a named dynamic collection

collection NAME --query EXPRESSION saves a named question after syntax validation. It stores the expression, not today’s file list. Running collection NAME later reevaluates it against the current catalog and personal annotations.

--remove deletes only that saved definition, not tracks or favorites. --query and --remove are incompatible. Without either, the command displays current tracks and supports the same CSV/JSON/M3U, pagination and sorting as query: title, artist, album, year, duration, size, rating, played, catalog (suffix - reverses).

Output, pagination and sorting options apply only when running the collection. They are refused with --query or --remove before changing its definition.

Use collections to find saved names, copy --collection NAME to put its current selection on a player, and play collection:NAME to listen locally. An unknown name is refused with guidance to create it; a syntactically invalid definition is not saved.

## Syntax and arguments

```text
aede collection <name>
```

One saved collection name; quote it if it contains spaces.

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
| `--sort ORDER` | Choose a supported column; append - for descending order, for example duration-. Supported values for this command are listed below. |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |
| `--query EXPRESSION` | Pass a relational query expression. Its projection depends on the command: save it, select tracks or retain matching albums/artists. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede collection Road --query "loved"
aede collection Road
aede collection Road --m3u --output road.m3u8
aede collection Road --remove
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[collections](collections.md), [query](query.md), [copy](copy.md), [play](play.md).

Detailed existing guide: [querying.md](../querying.md).
