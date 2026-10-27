#!/usr/bin/env bash
# Native musl CI needs C++ as well as C for the approved historical APE SDK.
# Ubuntu's musl-tools supplies no matching C++ standard library. Compile and
# run the fixtures in the official Rust/Alpine image on each native runner.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ $# -ne 1 ]]; then
    echo "Usage: tools/ci-musl.sh <native-musl-target>" >&2
    exit 2
fi
case "$(uname -m):$1" in
    x86_64:x86_64-unknown-linux-musl|aarch64:aarch64-unknown-linux-musl) ;;
    *) echo "Expected the matching native Linux x86_64/ARM64 musl runner" >&2; exit 2 ;;
esac
# The official multiarch image pins the declared Rust MSRV and Alpine branch;
# no SDK/compiler download is introduced into normal Cargo file decoding.
docker run --rm \
    --volume "$PWD:/source" --workdir /source \
    --volume "${CARGO_HOME:-$HOME/.cargo}/registry:/usr/local/cargo/registry" \
    --volume "${CARGO_HOME:-$HOME/.cargo}/git:/usr/local/cargo/git" \
    --env AEDE_REQUIRE_FFMPEG=1 --env CC=gcc --env CXX=g++ --env AR=ar \
    rust:1.89.0-alpine3.22@sha256:4b800f2e72e04be908e5f634c504c741bd943b763d1d8ad7b096cc340e1b5b46 sh -ec '
        apk add --no-cache build-base linux-headers git cmake autoconf automake libtool ffmpeg
        cargo test --locked --target "$1" --no-fail-fast
        cargo build --release --locked --target "$1"
    ' sh "$1"
