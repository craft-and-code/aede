# query — Select tracks with the relational query language

query selects tracks using Aède’s expression language. Space combines conditions with AND; OR and parentheses express alternatives; a leading - excludes a condition. Quote the whole expression so the shell does not interpret parentheses or comparison characters.

Text fields include title, artist, album, genre, label, comment, lyrics and path. Numeric fields support comparisons/ranges, for example year:1990..1999, duration:..4:00 and rating:>=4. Graph fields include recording, work, releasegroup, instrument, producer, composer, performing, guest and contributor. A precise nonexistent entity can return an error rather than an apparently valid empty result.

Bare rating/tag/note concern tracks. Use album.rating or artist.note for those levels. Bare loved inherits track, album or artist favorites; track.loved restricts it. Lyrics come from tags/.lrc sidecars. Trusted sourced graph links participate; pending/rejected claims remain evidence only.

Save recurring questions with collection. --m3u exports local track paths; --csv/--json return the selected rows; pagination affects that output. Supported sorting: title, artist, album, year, duration, size, rating, played and catalog, with a trailing - to reverse. The linked grammar guide is the authoritative field/operator reference.

Alias: `aede find`. Options and behavior are identical.

## Syntax and arguments

```text
aede query <expression>
```

One complete query expression. Prefer double quotes around it; escape any nested double quotes.

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

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede query "genre:jazz year:1950..1960"
aede query "loved played:0" --m3u --output unheard.m3u8
aede query "album.rating:>=4" --sort duration-
aede query "producer:Rick" --json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[collection](collection.md), [search](search.md), [copy](copy.md).

Detailed existing guide: [querying.md](../querying.md).
