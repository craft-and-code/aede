# serve — Serve the local HTTP/JSON/WebSocket catalog API and coordinate CLI writes

serve exposes the existing catalog on the same machine at http://127.0.0.1:8787. Scan at least one folder first: startup refuses without catalog.json. Keep the server and CLI on the same data directory. --port 0 selects a free port and prints it.

The current server provides HTTP JSON catalog routes and WebSocket catalog/activity notifications. It has no playback/streaming route, listener accounts or supported remote access. Other local users can read catalog metadata and paths. A NAS/Raspberry Pi deployment still needs target validation; no published Aède container image is provided.

On Unix, same-account store-writing CLI commands delegate to the server through its private socket. Closing that CLI does not stop accepted work; scan/fetch print task IDs for cancel. Ctrl-C or SIGTERM on the server stops new work and waits for accepted work. Setting a private AEDE_ADMIN_TOKEN of at least 32 ASCII characters before startup enables separate authenticated administration; a normal local CLI command does not need it. Read the server guide before configuring HTTP writes or services.

## Syntax and arguments

```text
aede serve [--port N]
```

No positional arguments. Start it in a terminal kept open, or use a service configuration appropriate to your system.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--port N` | Local HTTP port, integer 0–65535. Default 8787. 0 asks the system to choose a free port; read the printed address. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede scan "$HOME/Music"
aede serve
aede serve --port 0
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md), [cancel](cancel.md), [backup](backup.md).

Detailed existing guide: [operating.md](../operating.md).
