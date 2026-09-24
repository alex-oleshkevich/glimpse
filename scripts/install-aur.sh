#!/usr/bin/env bash
set -euo pipefail

read -ra elevate <<< "${GLIMPSE_ELEVATE:-}"

version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
pkg="$PWD/dist/glimpse-desktop-bin-${version}-1-x86_64.pkg.tar.zst"
if [[ ! -f "$pkg" ]]; then
    echo "no package at $pkg" >&2
    exit 1
fi

# dist/*.pkg.tar.zst survives between runs, so installing it directly (skipping `just
# package-aur`) silently reinstalls whatever was last built — this catches that before pacman
# does, rather than after, when the mismatch reads as a code bug.
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
bsdtar -xf "$pkg" -C "$scratch" usr/bin/glimpse-panel
built="$("$scratch/usr/bin/glimpse-panel" --version | sed -n 's/^commit_hash://p')"
head="$(git rev-parse --short=8 HEAD)"
if [[ "$built" != "$head" ]]; then
    echo "error: $pkg was built from $built, but HEAD is $head." >&2
    echo "run 'just install-aur' (not this script directly) to rebuild before installing." >&2
    exit 1
fi

"${elevate[@]}" pacman -U "$pkg"
echo "installed $pkg — 'just uninstall-aur' removes it"
