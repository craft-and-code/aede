# rules — List, export, or import reproducible personal decisions

rules lists reproducible human choices, exports them as a versioned bundle or merges an existing bundle. Included choices are source-review decisions, manual source records, individual credit exclusions, artist filing rules, set-aside missing releases, saved queries and relationship annotations.

This is deliberately different from backup: listening history, ordinary entity notes/favorites/ratings and remote biographies/artwork are not part of the rules bundle. Export/import your notes separately or use backup for the complete data snapshot.

--export writes to standard output or --output FILE. --import FILE merges the supported aede-rules document and reports added, updated, kept and imported source decisions. Export/import are mutually exclusive; --output without export is refused. Keep an ordinary backup before importing decisions you may want to reverse individually.

## Syntax and arguments

```text
aede rules
```

No positional arguments; --import supplies its filename.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--output FILE / -o FILE` | Write an export to FILE instead of standard output. A selection needs --csv, --json or --m3u; command-specific exports have their own rules. |
| `--export` | Export the command’s stored document. This is not the same as CSV/JSON presentation of a listing. |
| `--import FILE` | Merge a previously exported document from FILE; consult format-specific collision rules below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede rules
aede rules --export --output rules.json
aede rules --import rules.json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[backup](backup.md), [notes](notes.md), [review](review.md).

Detailed existing guide: [commands.md](../commands.md).
