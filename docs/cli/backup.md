# backup — Save the catalog, sources, and personal data together

backup creates one versioned bundle containing readable catalog, conclusions, personal data and source evidence. It can run even when only some stores exist; a data directory with personal notes but no catalog is still worth saving. The summary states which parts were held, empty or unreadable.

Choose an explicit filename and keep a copy off the machine/NAS. If that file already exists, Aède asks before overwriting it; --yes bypasses that prompt. A completely empty data directory returns an error rather than writing a misleading empty backup.

The bundle contains no original audio, artwork, lyrics sidecars or other derivative assets. Back those up separately. Treat the bundle as private because it can include personal history and absolute file paths. Use restore to recover stores, not export: an ordinary catalog export does not include all irreplaceable personal information.

## Syntax and arguments

```text
aede backup <file>
```

One destination file; use the positional filename, not --output.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--yes` | Skip the confirmation when this command asks for one. Inspect the intended operation first; use deliberately for unattended execution. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede backup aede-backup.aede
aede backup "/Volumes/Backup/aede-2026-10.aede"
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[restore](restore.md), [export](export.md), [rules](rules.md).

Detailed existing guide: [operating.md](../operating.md).
