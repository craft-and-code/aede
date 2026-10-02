# reset — Remove the catalog snapshot, including watched roots, while keeping separate stores

reset deletes catalog.json after presenting what it holds and asking for confirmation. It does not delete audio, sources.json, user.json or conclusions.json. Personal notes, ratings, favorites, fingerprints, integrity verdicts and imported analyses kept in those separate stores survive.

Watched folders live in catalog.json: they must be named again when rebuilding. The command prints a scan command with the previous roots. Copy it before closing the terminal. Despite an older help summary, reset does not preserve the roots inside a replacement catalog.

Make a backup before using reset if you need to recover the exact catalog snapshot. --yes accepts the deletion without a prompt; it does not make it reversible. If no catalog exists, reset reports that nothing needs removing.

## Syntax and arguments

```text
aede reset
```

No positional arguments.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--yes` | Skip the confirmation when this command asks for one. Inspect the intended operation first; use deliberately for unattended execution. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede backup before-reset.aede
aede reset
aede scan "$HOME/Music"
```

## Result and errors

Before confirmation, the table names tracks, albums, artists, watched folders and the catalog file being removed. After deletion, “catalog removed” is followed by a rebuild scan command containing the previous roots. Answering no reports “nothing was removed” and succeeds without deletion; a missing catalog also succeeds with an explanatory message. Failure to read a present catalog or remove its file returns an error. Rebuilding requires explicit roots and may also require reinstating exclusions; restore a backup to recover the exact previous catalog configuration.

## Related reading

[backup](backup.md), [scan](scan.md).

Detailed existing guide: [library.md](../library.md).
