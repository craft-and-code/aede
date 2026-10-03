# doctor — Find missing metadata, duplicates, and other catalog issues

doctor reports catalog problems without repairing anything automatically: missing tags, duplicates, incomplete releases, unresolved source identities, incomplete credits, stale or orphaned information and source disagreements. Severity groups are error, warning and info.

Read the reason and the adjacent command suggested by the report. A duplicate or artist-merge suggestion is a diagnostic, not authorization to delete files or merge people. Missing credits can mean unidentified recordings or lookups still waiting; use credits to distinguish them before refetching.

Use --severity to focus on one class and --all to inspect the complete report. --json preserves diagnostic structure. doctor operates on the catalog snapshot and held evidence; it does not contact metadata providers or check every audio byte.

Tag-based duplicate suggestions require a known artist, title and positive readable duration. Every duration in a suggested group is within three seconds of the shortest; different versions cannot be joined merely through intermediate durations. These remain suggestions for review. Missing track/disc reports show the first twelve positions and an ellipsis for longer lists, even when a malformed total tag announces billions of positions.

## Syntax and arguments

```text
aede doctor
```

No positional arguments.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--limit N` | Show at most N diagnostic details; use a positive whole number. Default: 25, for human and JSON output. Human summaries still count every filtered issue. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Show every row. Refused with --limit. Some commands also use it to include normally hidden categories, explained below. |
| `--json / -j` | Write the severity-filtered, paged diagnostic array. Use `--all` for the complete result; a page beyond the end is `[]`. |
| `--severity error\|warning\|info` | Keep only error, warning or info diagnostics. This filters the report; it does not fix anything. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede doctor
aede doctor --severity warning --all
aede doctor --json
```

## Result and errors

Human output counts all severity-filtered issues, groups them by kind, then shows the requested page of details. Control characters in metadata and paths are displayed literally; JSON retains their original values. Invalid windows are refused even when a filter leaves no issues. Read the diagnostic reasons; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[credits](credits.md), [review](review.md), [check](check.md), [merge](merge.md).

Detailed existing guide: [library.md](../library.md).
