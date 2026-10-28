#!/usr/bin/env bash
# Locked source verification only: no product App, frontend bundle or engine build.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# -ne 0 ]]; then
    echo "Usage: tools/check-sources.sh" >&2
    exit 2
fi
python3 tools/check_sources_tests.py

python3 tools/update_flaccompagnon_tests.py
python3 tools/project_stats_tests.py
python3 tools/gapless_tests.py
python3 tools/bit_perfect_tests.py
python3 tools/native_codec_tests.py
python3 tools/native_platform_tests.py
python3 tools/build_site_tests.py
python3 tools/desktop_release_tests.py
python3 tools/check-release.py
cargo fmt -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
if command -v ffmpeg >/dev/null 2>&1; then
    export AEDE_REQUIRE_FFMPEG=1
fi
cargo test --locked --offline
cargo test -p aede-audio --all-features --locked --offline
cargo test -p aede-audio --no-default-features --locked --offline
cargo test -p aede-audio --no-default-features --features decode --locked --offline
cargo test -p aede-dsp --no-default-features --locked --offline
source_target="${CARGO_TARGET_DIR:-target}"
python3 tools/accounts_terminal_tests.py --binary "$source_target/debug/aede"
python3 tools/playback_terminal_tests.py --binary "$source_target/debug/aede"
python3 tools/project-stats.py --tests --output "$source_target/project-stats.json"
python3 tools/build-site.py --check --project-stats "$source_target/project-stats.json"
RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline --no-deps --quiet
mkdir -p dist-site/docs/rust
cp -R "$source_target/doc/." dist-site/docs/rust/
python3 tools/check-site.py dist-site --require-rustdoc
echo "All source checks passed. App compilation, packaging and installed-engine coherence were not checked."
