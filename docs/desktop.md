# Aède desktop server

Aède 0.5.0 introduces two separate distributions: the existing **CLI** and an **App** that starts the same bundled CLI as a local HTTPS server. The application is a server console, not a second music player. Phémios connects to it using its normal server/account interface. Release builds remain unsigned. Until the 0.5.0 release is published, build the app from this checkout; the previous public release does not contain it.

## Run the application

Install the App package for your system from the [releases](https://github.com/craft-and-code/aede/releases). CLI archives have `CLI` in their names; desktop installers have `App`. Open **Aède**, add music folders, select a free port (8787 by default), click **Démarrer le serveur**, then **Scanner les dossiers**. Starting the server does not silently scan or fetch external metadata. Scan progress/result and cancellation use the existing [administrative tasks](server/jobs.md).

Folders and the port are edited while stopped. Registration is separate from scanning: a scan applies the current nonempty selection. Removing a folder never deletes music; removing all configured folders does not erase the retained catalog. An absent folder is retained during a scan. The first app detects paths that no longer exist; it cannot yet detect a network mount that has disappeared while leaving an accessible empty mountpoint. Remount such volumes before scanning.

The app listens only on `127.0.0.1`, always over HTTPS. An occupied port is an error, never a reason to adopt another server or fall back to HTTP. Remote/LAN hosting remains available through the [CLI HTTPS configuration](server/remote.md), outside this minimal local console.

## Connect Phémios on the same computer

1. Start Aède and export its local connection profile.
2. In Phémios, open **Paramètres → Serveur Aède → Importer la connexion de l’application Aède** and choose the exported JSON file.
3. In Aède, reveal the generated password. Enter it in Phémios with the displayed `aede-desktop` account, then connect.
4. After a scan, reload the known catalog in Phémios.

The export contains only the exact HTTPS address, username and public certificate. It contains no password, session token or TLS private key. Phémios admits public profiles only for HTTPS `127.0.0.1` with an explicit port and validates the supplied certificate for HTTP **and WSS**. Import only profiles exported by the Aède app you intend to trust. No system certificate installation, global trust override or TLS-verification bypass is required. Reimport after changing the server port or importing the profile in a new Phémios session.

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
