# doctor — Find missing metadata, duplicates, and other catalog issues

doctor reports catalog problems without repairing anything automatically: missing tags, duplicates, incomplete releases, unresolved source identities, incomplete credits, stale or orphaned information and source disagreements. Severity groups are error, warning and info.

Read the reason and the adjacent command suggested by the report. A duplicate or artist-merge suggestion is a diagnostic, not authorization to delete files or merge people. Missing credits can mean unidentified recordings or lookups still waiting; use credits to distinguish them before refetching.

Use --severity to focus on one class and --all to inspect the complete report. --json preserves diagnostic structure. doctor operates on the catalog snapshot and held evidence; it does not contact metadata providers or check every audio byte.

## Syntax and arguments

```text
aede doctor
```

No positional arguments.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write this command’s structured JSON result. With analyze, save report files instead of changing terminal output. |
| `--severity error\|warning\|info` | Keep only error, warning or info diagnostics. This filters the report; it does not fix anything. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede doctor
aede doctor --severity warning --all
aede doctor --json
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[credits](credits.md), [review](review.md), [check](check.md), [merge](merge.md).

Detailed existing guide: [library.md](../library.md).
