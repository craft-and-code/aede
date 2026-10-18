# Aède desktop instructions

Follow the root `CLAUDE.md` and `AGENTS.md`, and `../docs/desktop.md`. The desktop stack, strict TypeScript rules, IPC boundaries, protected storage and verification mirror Phémios. Keep the Rust launcher core independent of Tauri; Aède owns catalog, account, scan and playback semantics. Rust 1.90 is the desktop baseline; do not raise the CLI baseline implicitly.

Run `tools/check.sh` from this directory before app commits, plus the root gate when the engine changes. Keep tests in sibling `*_tests.rs` files. Preserve an occupied port as an explicit error, owner-private state, exact certificate verification, separate CLI/App artifact names and offline development startup. Never expose arbitrary shell/file/HTTP operations to the webview.
