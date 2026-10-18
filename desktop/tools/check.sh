#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
npm run format:check
npm run lint
npm test
npm run build
npm run docs:ts
npm run components:check
npm run check:tauri
node scripts/prepare-engine.mjs
cargo fmt --all -- --check
# The same fixed executable is used by packaging and the real HTTPS acceptance tests.
export AEDE_DESKTOP_TEST_BINARY="${AEDE_DESKTOP_BINARY:-$(node -e 'const p=require("node:path");process.stdout.write(p.resolve("..","target","release",process.platform==="win32"?"aede.exe":"aede"))')}"
cargo test --workspace --locked --offline --features aede-desktop-core/engine-tests
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked --offline
cargo build --workspace --locked --offline
