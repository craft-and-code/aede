# played — Manually record a listen made outside Aède, or undo its latest record with --remove

played records one manual listen for a catalogued track heard outside Aède. It is useful when another player cannot update this personal history. play already records its own listens, so do not manually duplicate them.

--remove takes back the latest recorded listen for the selected track; it does not delete the whole history. Read history afterward or query played:0 for tracks with no recorded listen. A play count is a personal record, not a claim about all listening on other services.

An unknown/ambiguous track must be resolved before a record can be added. Personal history lives in user.json and is included in backup, but excluded from the narrower rules bundle.

## Syntax and arguments

```text
aede played <track>
```

One catalogued track title.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--remove` | Undo or remove the personal value named by this command; the scope is described below. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede played "So What"
aede played "So What" --remove
aede history
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[history](history.md), [play](play.md), [query](query.md).

Detailed existing guide: [annotating.md](../annotating.md).
