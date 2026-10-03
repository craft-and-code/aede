# track — Show a track's album, credits, tags, and technical facts

track searches a local track title and presents its release position, file properties, credits, annotations and source evidence. --artist/--album/--comment narrow same-titled files before pagination. Put the title first: a name-valued option consumes following words until the next option.

--lyrics displays embedded or .lrc sidecar words; it does not download anything. --json separates local credits, sourced recording/work credits, exact-edition credits and parent-work provenance. Imported analyses are attributed rather than presented as fresh measurements.

JSON goes to standard output, or to the requested `--output` file while retaining the same detailed fields. All output formats use the same selected tracks after filtering and pagination.

Continue links open the underlying recording, works, album and credited artists. A recording is a performance identity; a track is its placement within an edition. CSV/M3U outputs use the matching local files. If too many matches appear, add artist/album restrictions instead of assuming the first is the intended edition.

## Syntax and arguments

```text
aede track <title>
```

Track title first, then name-valued filters.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--csv` | Write a CSV table of this command’s result, suitable for a spreadsheet; mutually exclusive with other output formats. |
| `--m3u` | Write an M3U playlist containing the selected local tracks. It contains paths, not copies of the audio. |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--artist NAME` | Filter by artist name on albums/track; on credit, name the credited person. Multiword names are accepted. |
| `--comment TEXT` | Filter local file comment text. Comments are embedded metadata, distinct from personal notes. |
| `--limit N` | Show at most N matching tracks; use a positive whole number. The default is 10. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--separator ";" / --separator tab` | With --csv only: use comma by default, a one-character separator such as ;, or tab for tab-separated text. Quote ; in a shell. |
| `--album TITLE` | Narrow a track title to the named album. Useful when multiple editions contain the same title. |
| `--lyrics` | On track, display lyrics; on search, search their text; on fetch, download missing .lrc sidecars from LRCLIB. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede track "So What" --artist "Miles Davis"
aede track "So What" --album "Kind of Blue" --lyrics
aede track "So What" --json --output track.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[album](album.md), [recording](recording.md), [fetch](fetch.md).

Detailed existing guide: [browsing.md](../browsing.md).
