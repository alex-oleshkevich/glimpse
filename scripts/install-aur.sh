#!/usr/bin/env bash
set -euo pipefail

read -ra elevate <<< "${GLIMPSE_ELEVATE:-}"

version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
pkg="$PWD/dist/glimpse-desktop-bin-${version}-1-x86_64.pkg.tar.zst"
if [[ ! -f "$pkg" ]]; then
    echo "no package at $pkg" >&2
    exit 1
fi
"${elevate[@]}" pacman -U "$pkg"
echo "installed $pkg — 'just uninstall-aur' removes it"
