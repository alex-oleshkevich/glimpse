#!/usr/bin/env bash
set -euo pipefail

mkdir -p dist
scripts/render-pkgbuild.sh --release "$1" "$2" > dist/PKGBUILD
