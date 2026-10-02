# merge — Record that two local artist spellings name one person

merge records your statement that one artist spelling should be filed under another. It is a personal reading rule stored in user.json, not a rewrite of tags or external metadata. The first spelling gives way to the second; quote each complete name separately.

The next scan rebuilds the graph around that rule. --list shows rules and whether they are already effective. --forget FIRST removes a rule; scan again to undo its catalog effect. Names not yet on the shelf may be stated before a later scan, with an explicit warning to reveal possible typos.

If both local spellings have different MusicBrainz artist identifiers, merge refuses: they identify different people and must be corrected deliberately at the source. doctor suggests candidate spelling pairs but never applies them. --source is only meaningful for the rule/source-related operation offered by the command; it is not a fuzzy auto-merge mode.

## Syntax and arguments

```text
aede merge <first> <second>
```

Ordinary action: exactly two separately quoted artist spellings. Removal: the spelling whose filing rule should be forgotten.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--forget` | Remove stored data/decisions in this command’s scope. It does not delete audio or retag files. |
| `--list` | List the individual stored records or decisions instead of the normal summary/operation. |
| `--source NAME` | Recognized by command scope, but the current merge handler does not apply this filter. Filing rules remain personal statements. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede merge "O. Osbourne" "Ozzy Osbourne"
aede scan
aede merge --list
aede merge --forget "O. Osbourne"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md), [doctor](doctor.md), [rules](rules.md).

Detailed existing guide: [library.md](../library.md).
