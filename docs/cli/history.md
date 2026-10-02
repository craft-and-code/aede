# history — Show listening history, newest first

history shows recorded listens newest first, alongside summary/count information. play records local playback automatically; played adds a listen made elsewhere. Pagination controls the displayed history, not the stored records.

--remove asks to clear the whole personal listening history, not merely the page shown. --yes skips that confirmation. This differs from played TITLE --remove, which takes back the latest listen for one track. History clearing does not delete music or notes.

There is no CSV/JSON/--output history export in the current CLI. Use backup if the history matters to you. It is private personal data, stored with annotations in user.json; the rules bundle deliberately excludes it.

## Syntax and arguments

```text
aede history
```

No positional arguments.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--yes` | Skip the confirmation when this command asks for one. Inspect the intended operation first; use deliberately for unattended execution. |
| `--remove` | Clear the whole listening history after confirmation; not only this result page. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede history --limit 20
aede history --offset 20 --limit 20
aede history --remove
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[played](played.md), [play](play.md), [backup](backup.md).

Detailed existing guide: [annotating.md](../annotating.md).
