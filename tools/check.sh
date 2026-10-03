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

echo "-> Build helper tests (offline)"
python3 tools/update_flaccompagnon_tests.py
python3 tools/project_stats_tests.py

echo "-> Website renderer and bilingual documentation"
python3 tools/build_site_tests.py

echo "-> Formatting"
cargo fmt --all -- --check

echo "-> Lint (no warning tolerated)"
cargo clippy --locked --offline --all-targets -- -D warnings

echo "-> Tests"
if command -v ffmpeg >/dev/null 2>&1; then
    export AEDE_REQUIRE_FFMPEG=1
    echo "   FFmpeg available: conversion regressions are required"
elif [[ "${AEDE_REQUIRE_FFMPEG:-}" == "1" ]]; then
    echo "FFmpeg is required for this verification run" >&2
    exit 1
else
    echo "   FFmpeg unavailable: optional conversion coverage will be skipped"
fi
cargo test --locked --offline

echo "-> Masked account entry in real pseudo-terminals"
stats_target="${CARGO_TARGET_DIR:-target}"
python3 tools/accounts_terminal_tests.py --binary "$stats_target/debug/aede"

echo "-> Generated project statistics and bilingual documentation"
python3 tools/project-stats.py --tests --output "$stats_target/project-stats.json"
python3 tools/build-site.py --check --project-stats "$stats_target/project-stats.json"

# Broken doc links are silent everywhere else: neither the build nor clippy
# reads them. Moving an item between modules is exactly what breaks them, and
# the documentation is where the reasoning behind this code lives.
echo "-> Documentation (no broken link)"
RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --no-deps --quiet

mkdir -p dist-site/docs/rust
cp -R "$stats_target/doc/." dist-site/docs/rust/
python3 tools/check-site.py dist-site --require-rustdoc

echo "-> Release build"
cargo build --locked --offline --release

echo
echo "All green."
