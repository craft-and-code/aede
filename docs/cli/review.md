# review — Resolve uncertain or conflicting source identities without changing tags

review lists uncertain name matches and conflicts between sourced identities and local tags. Each proposal has a stable ID. Read the local and sourced facts before accepting: acceptance lets that exact evidence participate in navigation and queries; rejection retains it as evidence only.

By default the list contains pending proposals. --all includes resolved ones as well. --interactive walks contextual cards in the terminal. --accept ID, --reject ID and --undo ID are separate direct actions: they refuse positional name filters, paging/source filters and interactive mode. --undo returns a decision to pending.

No tags are rewritten. Trust and source confidence remain separate: accepting a proposed match records your decision, not a new identifier read from the audio. Decisions persist in sources.json and travel through rules export/import. Invalid, ambiguous or unavailable IDs are refused. doctor helps locate cases needing attention.

Cards and decision messages display imported names, facts, paths and URLs literally, without executing terminal controls. List pagination is validated even when no claim needs review.

Reading the list takes no writer lock. Interactive review and direct decisions keep the lock while saving choices.

## Syntax and arguments

```text
aede review [name] [--interactive] | aede review --accept=<ID> | --reject=<ID> | --undo=<ID>
```

Optional name filter for the list only; direct decisions use IDs from its output. Replace CLAIM_ID.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--source NAME` | Filter by the stored source/tool name. Use the names printed by the corresponding listing. |
| `--accept ID` | Trust the exact claim ID for navigation and queries, keeping its source evidence. |
| `--reject ID` | Keep the exact claim as evidence but prevent it from becoming a trusted navigation/query relationship. |
| `--undo ID` | Undo the exact review decision (review) or restore an excluded sourced credit (credit). |
| `--interactive` | Review one contextual claim at a time in the terminal. Do not combine with --accept, --reject or --undo. |
| `--limit N` | Show at most N rows; use a positive whole number. Default limits depend on the page, usually 50. |
| `--offset N` | Skip N rows before showing the result; N starts at 0. Ordering remains deterministic. |
| `--all` | Include resolved decisions along with pending proposals, and lift the row limit. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede review
aede review --interactive
aede review --accept CLAIM_ID
aede review --undo CLAIM_ID
aede review --all
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[sources](sources.md), [doctor](doctor.md), [rules](rules.md).

Detailed existing guide: [sources.md](../sources.md).
