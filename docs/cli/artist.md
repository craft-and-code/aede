# artist — Show a local artist or a source-only credited contributor and their links

artist opens a local artist by name or MusicBrainz ID. Its card separates discography, guest appearances, compilation appearances, writing/production contributions and collaborations instead of placing every association in the same “album” list.

--role narrows credited contributions, --with selects shared tracks with another local artist and --members shows dated band membership. Local albums can derive the line-up appropriate to their year from sourced dates; this is not an invented historical lineup.

Choose one of --role, --with or --members; combinations are refused. On a local artist's ordinary card, --limit/--offset page each album table separately (50 rows by default) and the collaborators table (20 by default). With --role or --with, they page the track table; CSV/JSON/M3U also export that paged track selection. Ordinary track exports page the artist's performed tracks. --all removes these limits. Summary counts still describe the complete matching selection. --members is a complete dated page and refuses export and pagination options.

A contributor existing only in trusted recording/work/edition evidence has a source-only artist card. Use its exact MBID when a local namesake exists. --members and --with require local artists and are refused on that source-only card. CSV/M3U outputs select the relevant tracks, not biography prose. Source provenance remains visible, and the Continue commands navigate to adjacent graph objects.

## Syntax and arguments

```text
aede artist <name|MusicBrainz artist ID>
```

One artist name or MusicBrainz ID. Replace the example identifier.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--csv` | Write a CSV table of this command’s result, suitable for a spreadsheet; mutually exclusive with other output formats. |
| `--m3u` | Write an M3U playlist containing the selected local tracks. It contains paths, not copies of the audio. |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--role ROLE` | Keep the people or contributions with this credit role; on credit, specify the new role. |
| `--members` | Show dated band membership from trusted sourced information. Requires a local artist. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--separator ";" / --separator tab` | With --csv only: use comma by default, a one-character separator such as ;, or tab for tab-separated text. Quote ; in a shell. |
| `--with NAME` | Show tracks shared with another local artist. Requires a local artist on both sides. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede artist "Miles Davis"
aede artist "Miles Davis" --with "John Coltrane"
aede artist "A band" --members
aede artist MUSICBRAINZ_ARTIST_ID
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[artists](artists.md), [album](album.md), [relations](relations.md).

Detailed existing guide: [browsing.md](../browsing.md).
