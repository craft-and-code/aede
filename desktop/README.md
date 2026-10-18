# Aède desktop server

Local HTTPS server console for Phémios, with Serveur, Statut and bounded Journaux views. Folders can be edited live; only the listening port requires a stopped server. See the [English guide](../docs/desktop.md) or [French guide](../docs/fr/desktop.md) for operation, private profile storage, certificate import and platform limitations.

## Stack and layout

- `src/`: SolidJS, strict TypeScript, static CSS and one typed IPC boundary.
- `core/`: framework-independent Rust configuration, lifetime ownership and HTTPS administration. No Tauri dependency.
- `src-tauri/`: versioned commands, native directory/save dialogs, narrowly scoped clipboard writes and minimal local capabilities. No webview filesystem, shell, HTTP or clipboard-reading capability.
- `scripts/`: offline engine staging, dynamic Vite-port launch and locked Tauri bridge compatibility.
- `docs/components/`: isolated Storybook on loopback port 6007; public TypeScript contracts are generated separately.
- `tools/check.sh`: authoritative app verification. Rust unit tests are sibling `*_tests.rs` files; `engine-tests` runs against the exact bundled CLI.

Node 22.12/24/26+, Rust 1.90+ and the platform's [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) are required for development. CLI dependencies are fetched separately at the repository root; ordinary desktop development stays offline.

```sh
npm ci
cargo fetch --locked
npm run tauri dev
bash tools/check.sh
npm run build:native
```

The existing root check remains the engine gate. Native CI adds this workspace as a separate gate so CLI-only builds do not require WebKit/GTK/Node. No updater, background service, telemetry or network discovery is installed by the app.
