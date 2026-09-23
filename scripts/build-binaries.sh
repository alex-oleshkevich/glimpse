#!/usr/bin/env bash
set -euo pipefail

read -ra binaries <<< "${GLIMPSE_BINARIES:?set by the justfile; run through just}"

args=()
for b in "${binaries[@]}"; do args+=(-p "$b"); done
cargo build "$@" "${args[@]}"
