#!/usr/bin/env bash
# Everything that must pass before a commit.
#
#     tools/check.sh
#
# `--offline`: the dependencies are fetched once, then nothing here needs the
# network. A failure on that flag means a dependency was added without being
# discussed.

set -euo pipefail
cd "$(dirname "$0")/.."

echo "-> Formatting"
cargo fmt --all -- --check

echo "-> Lint (no warning tolerated)"
cargo clippy --locked --offline --all-targets -- -D warnings

echo "-> Tests"
cargo test --locked --offline

# Broken doc links are silent everywhere else: neither the build nor clippy
# reads them. Moving an item between modules is exactly what breaks them, and
# the documentation is where the reasoning behind this code lives.
echo "-> Documentation (no broken link)"
RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --no-deps --quiet

echo "-> Release build"
cargo build --locked --offline --release

echo
echo "All green."
