# label — Show a label's catalog and artists; check a Discogs profile when needed

label opens a label’s local releases/artists and shows whether its identity came from a tag, exact lookup, name proposal or conflicting source. Proposals/conflicts do not silently enter trusted navigation: review resolves them.

Unlike ordinary local pages, label may explicitly check a needed Discogs profile when opening the card. --offline prevents any network request; --online requests a fresh check and cannot be combined with --offline. Downloaded prose remains attributed, displayed as text with label/artist links; local tags are unchanged.

CSV/JSON/M3U export the related local track selection, not the whole remote profile. Use the displayed precise identity when names collide. fetch --labels retrieves MusicBrainz identities, while label’s profile check is a contextual fallback when Wikipedia prose is unavailable.

## Syntax and arguments

```text
aede label <name> [--offline]
```

One label name.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--online` | Check the label’s Discogs profile now instead of relying only on a held answer. |
| `--offline` | Open the label without any network check. Refused with --online. |
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
aede label "Blue Note" --offline
aede label "Blue Note" --online
aede fetch --labels
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[labels](labels.md), [review](review.md), [fetch](fetch.md).

Detailed existing guide: [browsing.md](../browsing.md).
