# restore — Restore a previously saved Aède backup

restore reads an Aède backup bundle and explains each store it can replace before asking for confirmation. A readable included store replaces its current counterpart. A store absent from the bundle or unsupported by this build is left untouched, never deleted. This is replacement, unlike notes/rules imports that merge.

Stop the server first for a production recovery. Save the current data directory/bundle before restoring so you can go back. For a rehearsal, use a separate empty --data directory and inspect its contents before touching production. --yes skips the confirmation deliberately.

The catalog is the snapshot taken on the backup date. Run scan afterward to reconcile added/removed music, with all original roots reachable. Restore cannot recover missing audio because audio is not in the bundle. An unreadable bundle or one with no supported restorable store is refused.

## Syntax and arguments

```text
aede restore <file>
```

One existing Aède backup file; not a music folder, CSV or FlacCompagnon report.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--yes` | Skip the confirmation when this command asks for one. Inspect the intended operation first; use deliberately for unattended execution. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede restore aede-backup.aede --data "/path/to/rehearsal-data"
aede restore aede-backup.aede
```

## Result and errors

Before confirmation, each store says whether it will replace current data, create a missing store or be left untouched. Completion counts restored stores, reports the catalog snapshot date and warns about watched roots absent on this machine. Mount those roots before scanning. Refusing confirmation returns “nothing was restored”; an unreadable bundle or no compatible stores is an error. Stores are written separately: an I/O failure after one write may leave part of the restoration applied. Keep a backup of the current data and stop concurrent writers first; this is not a single transaction across every store.

## Related reading

[backup](backup.md), [scan](scan.md).

Detailed existing guide: [operating.md](../operating.md).
