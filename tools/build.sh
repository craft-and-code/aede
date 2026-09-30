#!/usr/bin/env bash
# Online build entry point: select a published core, then use the offline gate.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ $# -gt 0 ]]; then
    echo "Usage: bash tools/build.sh"
    echo "Select the latest published FlacCompagnon Core, verify and build Aede."
    if [[ $# -eq 1 && ( "$1" == "--help" || "$1" == "-h" ) ]]; then
        exit 0
    fi
    exit 2
fi

python3 tools/update-flaccompagnon.py
cargo fetch --locked
bash tools/check.sh
