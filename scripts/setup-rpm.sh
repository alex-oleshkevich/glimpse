#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
version=0.21.1
checksum=94d79151322105715a0daaf2440a42eb5fd8a7793f04e92571ed910c4bd3f632
patch_file="$workspace/scripts/cargo-generate-rpm-directories.patch"
patch_checksum="$(sha256sum "$patch_file")"
identity="$checksum:${patch_checksum%% *}"
tool_root="$workspace/target/tools/rpm"

if [[ -x "$tool_root/bin/cargo-generate-rpm" && -f "$tool_root/source-id" ]] \
    && [[ "$(cat "$tool_root/source-id")" == "$identity" ]]; then
    exit 0
fi

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
archive="$scratch/source.crate"
curl -fsSL "https://static.crates.io/crates/cargo-generate-rpm/cargo-generate-rpm-$version.crate" -o "$archive"
printf '%s  %s\n' "$checksum" "$archive" | sha256sum -c -
tar -xzf "$archive" -C "$scratch"
source="$scratch/cargo-generate-rpm-$version"
patch --batch --fuzz=0 -p1 -d "$source" < "$patch_file"
cargo install --locked --path "$source" --root "$tool_root" --target-dir "$tool_root/build" --force
printf '%s\n' "$identity" > "$tool_root/source-id"
