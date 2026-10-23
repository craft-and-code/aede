# Aède desktop server

Aède 0.6.0 provides two separate distributions: the existing **CLI** and an **App** that starts the same bundled CLI as a local HTTPS server. The application is a server console, not a second music player. Phémios connects to it using its normal server/account interface. Release builds remain unsigned. App packages are built separately from CLI archives; build this checkout to use changes that are not yet published.

## Why this App exists

The desktop console makes the existing engine accessible without learning the CLI: select music folders, start a local encrypted server, check its status and inspect request logs. It is especially useful for testing Aède and Phémios as two separate apps on the same machine. It does not replace Phémios’s player or the headless CLI used on servers and NAS hosts. The App owns a separate data profile; connect Phémios using its exported public connection profile and the account shown in the console.

## Run the application

Install the App package for your system from the [releases](https://github.com/craft-and-code/aede/releases). CLI archives have `CLI` in their names; desktop installers have `App`. Open **Aède**, add music folders, select a free port (8787 by default), then click **Démarrer le serveur**. When folders are selected, startup launches an incremental background scan through the existing [administrative tasks](server/jobs.md); its progress, result and cancellation remain visible. Starting the server never fetches external metadata. **Scanner les dossiers** explicitly rescans later file additions or retries a failed/cancelled scan.

Folders can be added or removed while the server is running (outside an active app scan); a changed nonempty selection immediately launches a background replacement scan. While stopped, edits are saved for the next startup scan. Changing the port still requires stopping the server. Removing a folder never deletes music; removing all configured folders does not erase the retained catalog. An absent folder is retained during a scan. The App detects paths that no longer exist; it cannot yet detect a network mount that has disappeared while leaving an accessible empty mountpoint. Remount such volumes before scanning.

New folder selections are refused before saving if their combined serialized scan request would exceed the administrative request budget, including when all volumes are absent. The previous selection and connection identity are preserved. An older oversized profile remains readable. Remove existing folders one at a time until the selection fits; each strict removal is saved even if more removals are still needed. Scanning remains refused while the selection exceeds the budget, and adding or replacing folders does not bypass admission.

If an unavailable alias cannot be safely associated with its previously scanned paths, the scan refuses before publishing a replacement catalog. Reconnect the volume or select its real folder, then retry. Some combined changes, such as an unknown unavailable alias and other disappearing tracks, require this explicit recovery because the profile does not persist a historical alias-to-target map.

The app listens only on `127.0.0.1`, always over HTTPS. An occupied port is an error, never a reason to adopt another server or fall back to HTTP. Remote/LAN hosting remains available through the [CLI HTTPS configuration](server/remote.md), outside this minimal local console.

## Connect Phémios on the same computer

1. Start Aède and wait for the automatic scan of its selected folders to complete.
2. Export its local connection profile.
3. In Phémios, open **Paramètres → Serveur Aède → Importer la connexion de l’application Aède** and choose the exported JSON file.
4. In Aède, reveal the generated password. Enter it in Phémios with the displayed `aede-desktop` account, then connect.
5. A current Phémios detects catalogue publications in the background, whether produced by this App or a CLI command against the connected server's data directory. Older clients can use **Recharger le catalogue connu** after a scan. Remote folders are displayed read-only in Phémios; edit them in Aède. A client reload itself never scans files.

The export contains only the exact HTTPS address, username and public certificate. It contains no password, session token or TLS private key. Phémios admits public profiles only for HTTPS `127.0.0.1` with an explicit port and validates the supplied certificate for HTTP **and WSS**. Import only profiles exported by the Aède app you intend to trust. No system certificate installation, global trust override or TLS-verification bypass is required. Reimport after changing the server port or importing the profile in a new Phémios session.

## Status, request logs and browser entry

The app has three views: **Serveur** for configuration/scans, **Statut** for explicit authenticated HTTPS/API/catalog checks and counts, and **Journaux** for request summaries and process lifecycle. The status view distinguishes an alive process from an answering API and a loaded catalog. Its refresh button performs read-only probes, never scans. Copy icons put the login, active HTTPS address or generated password on the native clipboard; password copy does not reveal that password to the webview and marks it as sensitive for clipboard managers that honor the platform hint. The show/hide icon remains available beside it. These actions do not grant webview clipboard-reading access.

Opening `/` in a browser now shows a static connection/help page, not an administrative or playback website. It exposes no installation path, credential, account metadata or catalog. API endpoints remain authenticated when accounts are enabled. A browser may ask whether to trust the generated local certificate; Phémios instead verifies the exported certificate explicitly.

The app opts its owned engine into `AEDE_REQUEST_LOG=1`. Ordinary CLI servers can opt in explicitly with the same environment setting. Each `AEDE_HTTP` stderr record contains only a known HTTP method, registered route template (never resource IDs or query strings), status code and response-header latency in milliseconds. Early rejections and unmatched paths use a generic label. A bounded 128-entry writer queue keeps a slow log reader off the request workers; saturation may omit diagnostic summaries, so this is not an audit ledger or stream-completion record.

The native app drains stderr using bounded lines and keeps only validated summaries plus fixed lifecycle/diagnostic messages. Raw engine output is not forwarded to the webview; detailed scan output remains in the scan result. The latest 500 entries stay in memory across server stop/start, with incremental view updates, until clearing them or quitting the app. The download icon exports a snapshot of the retained entries as `aede-journaux.json`, using the native save dialog with Downloads as the proposed folder. Export includes only the same sanitized summaries, never raw stderr or credentials. No log file is created automatically, and no remote collection or telemetry is used.

The application contains trackpad gestures without horizontal navigation or elastic scrolling. Actual scrollable panels keep their normal scrolling. Interface text and images cannot be selected or dragged; editable fields retain native text editing. The login, address and password have dedicated copy actions.

## Data and lifetime

The app uses its OS application-data folder (`fr.craftandcode.aede`), under `server/`. It never silently adopts the ordinary CLI store or Phémios's embedded engine. An exclusive profile lock refuses a second launcher using the same state. `server.json` retains the selected roots, port, generated administrator secret and TLS identity; it must remain private (0700 directory/0600 file on Unix, inherited per-user ACLs on Windows). Secrets stay out of process arguments, readiness output and public status. Corrupt, oversized or exposed profiles are refused, not reset.

The account is an ordinary Aède administrator; its privileges and subsequent account changes remain governed by the existing engine. If its credentials are changed elsewhere, the launcher refuses its retained password instead of resetting that account. Advanced account administration stays in Phémios or the CLI.

The app owns only its child process, through the [desktop lifetime lease](server/desktop.md). Stopping or exiting closes that lease, waits briefly for shutdown and terminates that child if necessary. It installs no background service, changes no firewall setting and performs no global process search. A stopped scan can leave completed work saved; restart/scan explicitly rather than assuming an interrupted job succeeded.

## Development and verification

The independent [`desktop/`](../desktop/README.md) workspace uses the same SolidJS, strict TypeScript, Vite, Tauri 2, Prettier, Oxlint and Vitest stack as Phémios. Rust policies live in `desktop/core`; `desktop/src-tauri` contains only native wiring and dialogs. CLI/domain crates retain Rust 1.89, while the desktop workspace requires Rust 1.90 or later.

```sh
cd desktop
npm ci
cargo fetch --locked
npm run tauri dev
```

Engine preparation builds the exact repository CLI using `--locked --offline`; fetch CLI dependencies once from the repository root if missing. `AEDE_DEV_PORT` optionally chooses an exact Vite development port. Without it, the launcher discovers a free development port before opening its webview. `AEDE_DESKTOP_BINARY` can select an explicitly built engine; its reported version must match the app.

Run `bash tools/check.sh` inside `desktop/` before committing app changes, and the root `tools/check.sh` for engine changes. `npm run build:native` produces the host-native package. Release automation checks the engine and app versions, builds/test each on its native runner, and collects separate CLI/App artifacts into a draft GitHub release. macOS, Linux and Windows CI configuration does not replace manual acceptance on each platform.

Updating the engine is separate from scanning folders or synchronizing Phémios. Those actions use the running executable's API; they cannot add a capability to an old process. After installing/building a newer Aède App, restart the server/application, reconnect Phémios if its old session has expired, then explicitly synchronize the client catalog. Composer browsing requires the additive `dynamic_browse_composers` capability in that running engine, even when file tags already contain composer credits.
