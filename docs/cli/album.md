# album — Show one album's tracks, credits, and sources

album opens one local release/edition, showing ordered tracks, technical facts, local tags, credited people, personal annotations and separately attributed external claims. Album artwork and source links are contextual evidence; nothing is retagged by opening the page.

Use a MusicBrainz release ID to distinguish exact editions with the same title. The release group links equivalent album identities and other local editions. Classical work/grouping and movement tags stay separate from a sourced parent work; personnel remain separate from composition credits.

CSV/JSON/M3U exports cover the selected/paged tracks. --all includes every row before export. A singular album request must identify one release; ambiguous matches are refused with guidance rather than arbitrarily choosing a pressing. Continue commands open recordings, credited artists and the release group.

## Syntax and arguments

```text
aede album <title|MusicBrainz release ID>
```

One album title or exact MusicBrainz release ID.

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
aede album "Kind of Blue"
aede album MUSICBRAINZ_RELEASE_ID
aede album "Kind of Blue" --all --m3u --output album.m3u8
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[albums](albums.md), [release-group](release-group.md), [track](track.md).

Detailed existing guide: [browsing.md](../browsing.md).
