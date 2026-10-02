# import — Import acoustic analyses from FlacCompagnon

import reads [FlacCompagnon](https://craft-and-code.github.io/FlacCompagnon/) JSON reports, including reports saved at artist level covering several albums. It can precede the first scan: measurements attach by file path and wait until matching music is catalogued. The command does not perform a new acoustic analysis or contact the network.

--list shows attached, waiting and stale results. --pending narrows to results whose file is not yet catalogued. --forget removes the selected stored analyses; with --pending it removes only waiting results. Folder arguments narrow --list, --pending and --forget --pending; bare --forget refuses a folder argument. Ordinary import accepts report files or recursively walked report directories. --source narrows by tool name.

The newer available report date wins for overlapping file results, and changing source audio makes old measurements stale. Deleting a report later does not remove imported data. A malformed JSON file, unsupported report or unreadable path is reported rather than treated as audio. Use analyze when you want fresh in-process measurements.

## Syntax and arguments

```text
aede import <report…>
```

Ordinary mode: JSON reports or report directories walked recursively. List/pending/removal-of-pending: optional folders; bare --forget takes no folder.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--forget` | Remove stored data/decisions in this command’s scope. It does not delete audio or retag files. |
| `--pending` | Restrict imported analyses to results waiting for a matching scanned file. With --forget, remove only those waiting results. |
| `--list` | List the individual stored records or decisions instead of the normal summary/operation. |
| `--source NAME` | Filter by the stored source/tool name. Use the names printed by the corresponding listing. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede import "/path/to/artist-report.json"
aede import --list --all
aede import --pending
aede import --forget --pending
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[analyze](analyze.md), [scan](scan.md), [track](track.md).

Detailed existing guide: [imported-analyses.md](../imported-analyses.md).
