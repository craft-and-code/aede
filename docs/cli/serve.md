# serve — Serve the catalog and authenticated audio, and coordinate CLI writes

serve exposes the existing catalog on the same machine at http://127.0.0.1:8787. Scan at least one folder first: startup refuses without catalog.json. Keep the server and CLI on the same data directory. --port 0 selects a free port and prints it.

The server provides JSON catalog routes, WebSocket notifications and an [authenticated audio contract](../server/playback.md). [Accounts](accounts.md) protect catalog access and isolate personal data; without accounts, other local users can read catalog metadata and paths. Explicit [HTTPS configuration](../server/remote.md) enables remote access with mandatory accounts. A NAS/Raspberry Pi deployment still needs target validation; no published Aède container image is provided.

On Unix, same-account store-writing CLI commands delegate to the server through its private socket. Closing that CLI does not stop accepted work; scan/fetch print task IDs for cancel. Ctrl-C or SIGTERM on the server stops new work and waits for accepted work. Setting a private AEDE_ADMIN_TOKEN of at least 32 ASCII characters before startup enables separate authenticated administration; a normal local CLI command does not need it. Read the server guide before configuring HTTP writes or services.

An account administrator can also authorize installation jobs on local HTTP. HTTPS disables the entire `/api/admin` family and the legacy token; use the trusted local CLI for administration. [Account sessions](../server/accounts.md) use a bearer header for catalog reads and `/api/me/v1` personal data. Account CLI commands take the shared writer lock directly rather than delegating.

## Syntax and arguments

```text
aede serve [--bind IP] [--port N] [--tls-cert PATH --tls-key PATH --authority HOST:PORT]
```

No positional arguments. Start it in a terminal kept open, or use a service configuration appropriate to your system.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--port N` | Listening port, integer 0–65535; default 8787. HTTP allows 0 for a free port. TLS refuses 0. |
| `--bind IP` | Listening IP literal; default `127.0.0.1`. Non-loopback requires TLS. |
| `--tls-cert PATH` | PEM certificate chain. Requires `--tls-key` and `--authority`. |
| `--tls-key PATH` | Matching protected, unencrypted PEM private key. |
| `--authority HOST:PORT` | Exact public HTTPS authority, without scheme/path; bracket IPv6. It can differ from the listening address/port. |

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede scan "$HOME/Music"
aede serve
aede serve --port 0
aede serve --bind 0.0.0.0 --port 8787 --tls-cert /private/aede/fullchain.pem --tls-key /private/aede/key.pem --authority music.example:8787
```

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[scan](scan.md), [cancel](cancel.md), [backup](backup.md), [HTTPS configuration](../server/remote.md), [audio contract](../server/playback.md).

Detailed existing guide: [operating.md](../operating.md).
