# Operating the local server

Aède's M2 server is a **local catalog API**, not yet a remote music server. It serves JSON and WebSocket notifications, but it does not stream audio or authenticate listeners. The supported catalog workflow is macOS or Linux. Native Windows CI validates scanning, sidecars and copying; local Unix-socket delegation remains unavailable on Windows. See [Paths](design/paths.md).

## Start and stop

Choose one persistent data directory and use it for both CLI commands and the server. Set `AEDE_HOME` in the service environment, or pass `--data <directory>` consistently to every command. Without either override, Aède uses `$XDG_DATA_HOME/aede`, then `~/.local/share/aede` (`.aede` in the current directory if `HOME` is unset). The service account needs read access to the music folders and read/write access to its data directory; audio files and tags are never rewritten. Delegated commands that create files beside the music, such as `fetch --lyrics`, image downloads or `analyze --json`, also need write access to their destination folders. Those writes run with the server account's permissions.

```sh
aede scan /path/to/music
aede serve
```

The first scan creates `catalog.json`. `serve` refuses to start without it. The server prints its actual address when ready. It listens on `127.0.0.1:8787` by default; `--port` accepts values from 0 to 65535 and `aede serve --port 0` chooses and prints a free local port. A quick local check is `curl http://127.0.0.1:8787/api/v1/status`. Stop a foreground server with Ctrl-C; a service manager can send SIGTERM. Shutdown stops new connections, closes incomplete command requests, and waits for accepted operations to finish. A long delegated `fetch` or `check` can delay exit; cancel a delegated scan or fetch before stopping the server if it should not finish. Allow enough shutdown time in the service manager and stop gracefully before shutting down the NAS. The [API contract](api.md) lists endpoints, events and errors; `aede help serve` provides an offline command-line guide.

The HTTP catalog API is read-only. The [server README](../crates/aede-server/README.md) lists every available route and its parameters, including albums, artists, origins, tracks, diagnostics and search. After upgrading the executable, restart the server to activate new routes.

To allow separate administrative scan/fetch jobs and personal-data writes, supply a private ASCII `AEDE_ADMIN_TOKEN` of at least 32 characters in the server's environment **before** starting it. Keep the token in the service manager's secret facility or another protected local configuration, not in a URL, browser page or published example. `POST /api/admin/v1/scan` or `/api/admin/v1/fetch` with a JSON object returns a task ID; poll `GET /api/admin/v1/tasks/{id}` and cancel with an empty `POST /api/admin/v1/tasks/{id}/cancel`. The same token protects annotation, listening-history and smart-collection routes; these use the existing single `local` owner until accounts are implemented. All require `Authorization: Bearer <token>` and refuse browser origins; without the token these routes are absent. For compatibility, a scan with no body still runs synchronously. A normal same-account CLI scan needs no administrative token.

HTTP jobs survive client disconnection and graceful shutdown waits for them. Their IDs and bounded stdout/stderr are retained only in memory (at most four active jobs and 64 records), not across restart. Cancellation preserves already-saved work. Fetch uses the server's configured service keys and cannot prompt for input; explicitly supply `yes` when accepting a large run. Scan folders and fetch folder targets refer to paths on the server. HTTP polling/cancellation covers only JSON-submitted HTTP jobs; use `aede cancel` for delegated CLI tasks. Do not blindly retry a submission whose response was lost: the accepted job may still be running.

New image and lyric sidecars are published atomically without replacing an existing file, including one created concurrently by another program. Their destination filesystem must support hard links (for example, FAT/exFAT does not); otherwise the download reports an error instead of using an unsafe overwrite fallback. A failed or interrupted download never publishes a partial final sidecar, though an interruption can leave a hidden temporary file.

## CLI alongside the server

On Unix, write-capable CLI commands for the same account and data directory automatically run under the active server over a private local socket. They keep their usual output; a long `fetch` continues if its CLI disconnects. Delegated `scan` and `fetch` print a task ID. Use `aede cancel <task-id>` with the same `--data` or `AEDE_HOME` to request explicit cancellation; closing the terminal or pressing Ctrl-C on the CLI does **not** cancel the server task. Cancellation returns immediately and the original CLI exits with code 130 once stopped. It does not roll back answers already saved by `fetch`. Task IDs are valid only until the server restarts; finished tasks, HTTP administrative scans and other delegated commands cannot be cancelled this way. If no server is running, commands run locally and Ctrl-C stops that local process; `cancel` then reports that no server is available.

The data directory must not be writable by other users or groups. The private command socket uses a protected directory under `/tmp`; keep `.aede-server.lock` in place as well as the data lock. The server limits concurrent local command connections and refuses new requests when saturated. An incomplete request expires, and a CLI that stops reading output is disconnected after a timeout while an accepted command continues. Retry a refused request after other commands finish; a disconnected accepted command must not be blindly resubmitted, because it may still be running.

All current Aède writers coordinate through a lock in the data directory, including CLI commands with no server. Do not delete `.aede.lock`, hand-edit JSON while Aède is running, or mix old executables with the current server. A store held by another command makes a synchronous HTTP scan or doctor request return `409 store_busy`; asynchronous HTTP jobs wait for the lock. The server notices an externally replaced `catalog.json` in about a second. A malformed replacement leaves the last good snapshot available and logs an error; removing the catalog makes catalog routes unavailable until it is restored.

## Backups and recovery

Back up the data directory on persistent storage. It contains `catalog.json` (rebuildable graph), `conclusions.json` (integrity results and imported analyses), `user.json` (personal annotations), `sources.json` (attributed external information), and derivative assets. `aede backup <file>` produces a versioned bundle of the JSON stores; keep copies off the NAS. The bundle is **not** a backup of the original music or derivative image and lyric files. Treat both the data directory and bundle as private: they can contain listening history, file paths and fetched data.

For recovery, stop the server, preserve the damaged data directory, and run `aede restore <file>` with the same `AEDE_HOME` or `--data`. Review the command's confirmation before accepting it. Restore writes only stores present and readable in the bundle; it does not delete an absent store. Start the server again and check `/api/v1/status` and `/api/v1/library`. Legacy embedded conclusions are carried into `conclusions.json` on the next protected catalog save or when restoring a version-1 backup; reading a legacy catalog does not rewrite files. Keep a backup before upgrading or restoring. The original audio files must be backed up separately.

## Docker image on a Linux host (procedure only)

This is a vendor-neutral deployment procedure for a **future** Linux container image. The repository does not yet build, publish or test an Aède image, so `aede:local` below is a placeholder, **not** an image that can be pulled today. The example assumes that image contains a compatible `/usr/local/bin/aede` executable. The current release workflow builds Linux `x86_64`, not Linux `aarch64`; the image must match the host CPU. No NAS-specific package or service integration is required by this procedure.

Prepare existing, persistent host directories for music, Aède data and backup bundles. Replace the example absolute paths and numeric account with paths and a non-admin UID/GID that can traverse/read the music and exclusively write the data and backup directories. The data directory must not be writable by other users or groups. Keep `/music` and `/data` as the **same container paths** on every run: the catalog stores paths, and changing them makes tracks appear missing. A read-only music mount supports scanning and catalog reads; sidecar-writing commands need a deliberately writable music mount instead.

```sh
AEDE_IMAGE='aede:local'                 # replace with a real, trusted image tag
AEDE_MUSIC='/absolute/path/to/music'   # already exists on the Docker host
AEDE_DATA='/absolute/path/to/aede-data'
AEDE_BACKUPS='/absolute/path/to/aede-backups'
AEDE_UID_GID='1000:1000'               # replace with the account that owns the directories

docker run --rm --user "$AEDE_UID_GID" \
  -e AEDE_HOME=/data \
  --mount "type=bind,src=$AEDE_MUSIC,dst=/music,readonly" \
  --mount "type=bind,src=$AEDE_DATA,dst=/data" \
  --entrypoint /usr/local/bin/aede "$AEDE_IMAGE" scan /music

docker run -d --name aede --restart unless-stopped \
  --stop-signal SIGTERM --stop-timeout 300 \
  --network host --user "$AEDE_UID_GID" -e AEDE_HOME=/data \
  --mount "type=bind,src=$AEDE_MUSIC,dst=/music,readonly" \
  --mount "type=bind,src=$AEDE_DATA,dst=/data" \
  --mount "type=bind,src=$AEDE_BACKUPS,dst=/backups" \
  --entrypoint /usr/local/bin/aede "$AEDE_IMAGE" serve
```

On Linux, `--network host` makes Aède's existing `127.0.0.1:8787` listener reachable from processes on the Docker host (including other processes sharing its network namespace), not from other machines. Do not substitute a bridge network and `-p`: Aède would still listen on the container's own loopback, and Docker's port mapping would not reach it. Host networking does not use `-p`. Check `http://127.0.0.1:8787/api/v1/status` and `/api/v1/library` on the host, plus `docker logs aede`. These commands are for Docker Engine on Linux; Docker Desktop and other container runtimes need separate validation. See Docker's [host-network](https://docs.docker.com/engine/network/drivers/host/) and [bind-mount](https://docs.docker.com/engine/storage/bind-mounts/) documentation.

Run cooperating CLI commands **inside the running container** so they share its account, `/data`, and private `/tmp` command-socket namespace; another container with only the data mount does not share that socket. For example, `docker exec aede /usr/local/bin/aede stats` or `docker exec aede /usr/local/bin/aede backup /backups/aede-backup.aede`. Copy backup bundles off the host/NAS; they do not contain the original audio or derivative sidecars, which need separate backups. Do not put an admin token in the image or command line. If the administrative API is needed later, follow the token guidance above and restrict who can inspect the container's environment. Do not expose the HTTP port remotely.

Before relying on the deployment, stop and start the container with `docker stop aede` and `docker start aede`, checking that the same catalog returns. Rehearse `aede restore` with the bundle mounted read-only into a **different empty data directory** and the server stopped for any production restore. Then test an actual host reboot and an off-host backup/restore. `--stop-timeout 300` gives accepted work time to finish, but a longer job may still need explicit cancellation or a longer shutdown window. Docker's [restart policy](https://docs.docker.com/engine/containers/start-containers-automatically/) can restart a container that was running before a host reboot; it does not prove that the mounted music or data paths were ready, so verify both after reboot.

On 26 September 2026, a disposable macOS **non-container** rehearsal verified scan, loopback serving, backup, graceful restart and restore into a separate folder; the source audio checksum was unchanged. **No Docker image, container lifecycle, NAS volume, account permissions, off-host backup or NAS reboot has been validated.**

Do not publish port 8787, forward it through a router, or expose it through a reverse proxy/tunnel as a way to listen from a phone. Loopback access is not per-user authentication, `/api/v1` reveals catalog metadata and paths, and the administrative token does not make remote HTTP safe. Accounts, authorization, encrypted remote transport and audio playback are later work. The current server cannot satisfy the remote-phone listening scenario yet.
