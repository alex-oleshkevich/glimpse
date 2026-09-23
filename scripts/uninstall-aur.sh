#!/usr/bin/env bash
set -euo pipefail

read -ra elevate <<< "${GLIMPSE_ELEVATE:-}"

if ! pacman -Qq glimpse-desktop-bin >/dev/null 2>&1; then
    echo "glimpse-desktop-bin is not installed"
    exit 0
fi
"${elevate[@]}" pacman -R glimpse-desktop-bin
