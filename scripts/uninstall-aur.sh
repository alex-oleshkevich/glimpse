#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${GLIMPSE_SUDO:-}" ]]; then
    read -ra elevate <<< "$GLIMPSE_SUDO"
elif [[ "$(id -u)" == 0 ]]; then
    elevate=()
else
    elevate=(sudo)
fi

if ! pacman -Qq glimpse-desktop-bin >/dev/null 2>&1; then
    echo "glimpse-desktop-bin is not installed"
    exit 0
fi
"${elevate[@]}" pacman -R glimpse-desktop-bin
