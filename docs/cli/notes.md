# notes — List, export, import or reattach personal data

notes lists personal notes and can filter entities carrying --tag LABEL. CSV/JSON presentation exports the shown rows with pagination. --export instead writes the complete portable personal document; --import reads that format and merges it with current data rather than wiping the store.

On collisions, newer notes win and replaced values are reported; play counts retain the larger count. Importing the identical document twice is idempotent. The export is broader than plain Markdown note text because it preserves the personal annotation structure needed to attach data again.

Use note KIND NAME to read/write one exact note, backup for all four stores, and rules for reproducible source/graph decisions. Keep exports private because notes and file-based references can be personal.

## Syntax and arguments

```text
aede notes
```

No positional arguments; --import supplies a portable personal JSON file.

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
| `--waiting` | List unresolved personal references, including listening records and relationship endpoints. |
| `--relink REFERENCE --to KIND:NAME` | Explicitly attach a waiting reference to an existing target of the same kind; a unique name or exact destination reference is accepted, with ambiguity/conflicts refused. |
| `--dry-run` | Preview and validate a relink or undo without writing personal data. |
| `--relinks` | List relink decisions and their undo identifiers. |
| `--undo-relink ID` | Reverse an unchanged relink; refuse later edits and conflicts. |
| `--export` | Write the complete portable personal-data document, not just the currently paged note rows. |
| `--import FILE` | Merge a supported personal JSON export: newer note wins, larger play count wins; duplicate import is idempotent. |
| `--tag LABEL[,LABEL]` | On notes/relations, filter a personal label; on relation, add comma-separated labels, or remove them with --remove. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede notes --tag jazz
aede notes --export --output personal.json
aede notes --import personal.json
aede notes --waiting
aede notes --relink "track:/old/01.flac" --to "track:So What" --dry-run
aede notes --relinks
aede notes --undo-relink 1
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

Copy the exact source reference from --waiting. --to accepts kind:name, such as "album:Legion", or an exact reference; quote values containing spaces. Undo preserves original note text and prevents later automatic relocation of that old track reference. --dry-run requires --relink or --undo-relink; import/export, waiting, history and relinking are distinct modes. Listing filters are refused on writes/exports. Waiting references do not accept --search; reattachment history accepts neither --tag nor --search.

## Related reading

[note](note.md), [backup](backup.md), [rules](rules.md).

Detailed existing guide: [annotating.md](../annotating.md).
