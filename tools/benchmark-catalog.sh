#!/usr/bin/env bash
# Measures the current JSON catalog with a generated graph and fresh load processes.

set -euo pipefail
cd "$(dirname "$0")/.."

rounds="${AEDE_BENCH_RUNS:-3}"
if [[ ! "$rounds" =~ ^[1-9][0-9]*$ ]]; then
  echo "AEDE_BENCH_RUNS must be a positive integer" >&2
  exit 2
fi

if (( $# == 0 )); then
  sizes=(10000 50000 100000 200000)
else
  sizes=("$@")
fi
for size in "${sizes[@]}"; do
  if [[ ! "$size" =~ ^[1-9][0-9]*$ ]]; then
    echo "track counts must be positive integers: $size" >&2
    exit 2
  fi
done

if [[ ! -x /usr/bin/time ]]; then
  echo "the system /usr/bin/time is required for peak memory measurements" >&2
  exit 1
fi
case "$(uname -s)" in
  Darwin) time_option=-l ;;
  Linux) time_option=-v ;;
  *) echo "peak memory parsing is supported on macOS and Linux only" >&2; exit 1 ;;
esac

cargo build --offline --release -p aede-core --example catalog_bench >&2
bench_bin=target/release/examples/catalog_bench
bench_dir="$(mktemp -d "${TMPDIR:-/tmp}/aede-benchmark.XXXXXX")"
catalog_path="$bench_dir/catalog.json"
cleanup() {
  rm -f -- "$catalog_path" "$bench_dir/catalog.json.tmp"
  rmdir -- "$bench_dir"
}
trap cleanup EXIT

field() {
  local name="$1"
  sed -n "s/^${name}=//p" | head -n 1
}

rss_bytes() {
  if [[ "$time_option" == -l ]]; then
    awk '/maximum resident set size/ {print $1; exit}'
  else
    awk '/Maximum resident set size \(kbytes\)/ {printf "%.0f\n", $NF * 1024; exit}'
  fi
}

echo 'phase,tracks,run,build_ms,save_ms,load_ms,catalog_bytes,peak_rss_bytes'
for size in "${sizes[@]}"; do
  generation="$(/usr/bin/time "$time_option" "$bench_bin" generate "$size" "$catalog_path" 2>&1)"
  build_ms="$(printf '%s\n' "$generation" | field build_ms)"
  save_ms="$(printf '%s\n' "$generation" | field save_ms)"
  bytes="$(printf '%s\n' "$generation" | field catalog_bytes)"
  peak="$(printf '%s\n' "$generation" | rss_bytes)"
  echo "generate,$size,1,$build_ms,$save_ms,,$bytes,$peak"

  for ((run = 1; run <= rounds; run++)); do
    loaded="$(/usr/bin/time "$time_option" "$bench_bin" load "$catalog_path" 2>&1)"
    loaded_count="$(printf '%s\n' "$loaded" | field tracks)"
    if [[ "$loaded_count" != "$size" ]]; then
      echo "catalog size changed during benchmark: expected $size, got $loaded_count" >&2
      exit 1
    fi
    load_ms="$(printf '%s\n' "$loaded" | field load_ms)"
    peak="$(printf '%s\n' "$loaded" | rss_bytes)"
    echo "load,$size,$run,,,$load_ms,$bytes,$peak"
  done
  rm -f -- "$catalog_path"
done
