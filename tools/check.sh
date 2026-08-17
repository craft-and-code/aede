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
cargo clippy --offline --all-targets -- -D warnings

echo "-> Tests"
cargo test --offline

echo "-> Release build"
cargo build --offline --release

echo
echo "All green."
