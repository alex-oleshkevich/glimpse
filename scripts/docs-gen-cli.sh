#!/usr/bin/env bash
set -euo pipefail

read -ra binaries <<< "${GLIMPSE_DOCS_CLI:?set by the justfile; run through just}"

packages=()
for binary in "${binaries[@]}"; do
    packages+=(-p "$binary")
done
cargo build "${packages[@]}"

node website/scripts/cli.mjs target/debug "${binaries[@]}"
