# Start the local server

Aède's server gives other programs access to your local music **catalog**: albums, artists, tracks, relationships and diagnostics. It answers HTTP requests with JSON and publishes change notifications over WebSocket. Optional [accounts and sessions](accounts.md) protect access and isolate personal data. Audio transmission and remote playback remain future work.

## Before starting

Install Aède and scan at least one music folder. The CLI and server must use the same persistent data folder. Aède reads the original music and tags without rewriting them.

```sh
aede scan /path/to/music
aede serve
```

Replace `/path/to/music` with your music folder. Keep the second terminal open: `serve` runs until you stop it. Starting without a saved catalog fails; perform the scan first. The server prints its address when ready.

The default address is `http://127.0.0.1:8787`. `127.0.0.1`, also called loopback, means **this computer**. On a phone it means the phone, not your music computer. Aède deliberately listens only on IPv4 loopback.

```sh
curl http://127.0.0.1:8787/api/v1/status
```

`curl` is a command-line HTTP client. This request should return a JSON object whose `status` is `ok` and `catalog_loaded` is `true`. If your shell cannot find `curl`, install your operating system's HTTP client or use a local API client. A browser can display this public JSON URL on the same computer; the administrative interface has different restrictions.

## Keep one data location

With a custom data folder, repeat the global `--data` option for both commands:

```sh
aede --data /path/to/aede-data scan /path/to/music
aede --data /path/to/aede-data serve --port 3412
```

Alternatively set `AEDE_HOME` in the environment of both processes. Without an override, Aède uses `$XDG_DATA_HOME/aede`, then `~/.local/share/aede`; if `HOME` is absent, it uses `.aede` in the current folder. Music paths belong to the computer running the server.

`--port` accepts 0–65535. Port 0 asks the operating system for an available port; read the printed address instead of assuming 8787. After upgrading the executable, restart the running server to use the new version.

## Stop and work alongside the server

Use Ctrl-C in the server terminal for a graceful stop. A service manager can send SIGTERM. Accepted jobs can keep shutdown waiting; explicitly cancel a long scan or fetch before shutdown when you do not want it to finish. Closing an HTTP client does not cancel an accepted job.

On Unix, store-changing CLI commands using the same account and data folder automatically delegate to the server through a private local socket. They keep their usual output. Delegated scan/fetch tasks print an ID; `aede cancel <task-id>` requests their cancellation. HTTP task IDs are polled and cancelled through HTTP instead; these two interfaces are not interchangeable. Local socket delegation is unavailable on Windows.

Do not delete lock files or edit live JSON stores. CLI and server writers coordinate through the same data-folder lock. See [HTTP jobs](jobs.md) and the [operating guide](../operating.md) for accepted-job lifetime and backups.

## NAS, Raspberry Pi and containers

The architecture separates music, persistent catalog data and clients, making a small always-on host a relevant deployment target. Current support is a **local catalog workflow**, not a tested NAS appliance. No Aède container image is built or published; NAS packages, Raspberry Pi deployment, reboot handling and target-device performance still need validation. Published Linux release builds currently target x86_64, not Linux aarch64.

The [operating guide](../operating.md#docker-image-on-a-linux-host-procedure-only) describes a future Linux-container procedure, including stable paths, permissions and backups. Its placeholder image is not something you can pull today. Do not expose port 8787 through a router, reverse proxy or tunnel: the current API has no remote listener authentication or encrypted transport.

Next: [Understand HTTP and JSON](http.md), then [Browse albums and tracks](catalog.md).
