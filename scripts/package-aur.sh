#!/usr/bin/env bash
set -euo pipefail

version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
asset="glimpse-${version}-x86_64.tar.zst"
dest="$PWD/dist"
build="$dest/aur"
rm -rf "$build"
mkdir -p "$build"
cp "$dest/$asset" "$build/"
scripts/render-pkgbuild.sh --local "$version" > "$build/PKGBUILD"
# --nodeps: package() only copies an already-built tree, so the runtime
# dependencies are what the package declares, not what building it needs.
cd "$build" && PKGDEST="$dest" makepkg --force --nodeps --noconfirm
