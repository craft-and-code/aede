# search — Search graph objects and source-only contributors, plus optional notes, comments, or lyrics

search finds catalog artists, albums, tracks, recordings, works and release groups by free text. It also keeps source-only contributors and parent works separate from local identities. Each result provides an Open command, so follow a precise ID when names collide.

Comments, personal notes and lyrics are opt-in with --comments, --notes and --lyrics. Lyrics search shows matching lines rather than flooding the screen with entire songs. --json includes match origin (found_in) for prose matches. Search only reads stored/source-sidecar information; it does not fetch missing biographies or lyrics.

CSV/M3U export only track hits, not every artist/album result. They combine name, optional comment and optional lyric hits, remove duplicate tracks, then apply the requested offset and limit. Artist or album matches do not consume that track window. JSON and human output page each result category separately; JSON retains match origins and honours --output, including an empty array when nothing matches. Use query when you need numeric ranges, OR, exclusions or graph-role criteria rather than a plain text match.

## Syntax and arguments

```text
aede search <text>
```

Free-text search words. Quote multiword text.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--csv` | Write a CSV table of this command’s result, suitable for a spreadsheet; mutually exclusive with other output formats. |
| `--m3u` | Write an M3U playlist containing the selected local tracks. It contains paths, not copies of the audio. |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--comments` | Also search local comment tags; disabled by default. |
| `--notes` | Also search your personal notes; disabled by default. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--separator ";" / --separator tab` | With --csv only: use comma by default, a one-character separator such as ;, or tab for tab-separated text. Quote ; in a shell. |
| `--lyrics` | On track, display lyrics; on search, search their text; on fetch, download missing .lrc sidecars from LRCLIB. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede search coltrane
aede search remaster --notes --comments
aede search train --lyrics
aede search coltrane --json --output search.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[query](query.md), [artist](artist.md), [track](track.md).

Detailed existing guide: [querying.md](../querying.md).
