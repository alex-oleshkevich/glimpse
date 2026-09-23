#!/usr/bin/env bash
set -euo pipefail

raw_tag="${1:?usage: release-verify.sh TAG}"
version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"
tag="${raw_tag#v}"
if [[ "$tag" != "$version" ]]; then
    echo "tag ${raw_tag} does not match Cargo.toml version $version" >&2
    exit 1
fi
echo "tag ${raw_tag} matches Cargo.toml version $version"
