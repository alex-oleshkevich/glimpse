#!/usr/bin/env bash
set -euo pipefail

source_root="${1:-glimpse-applets}"
dest_root="${2:?usage: scripts/build-glimpse-applets.sh <source-root> <dest-root>}"

if [[ ! -d "$source_root" ]]; then
    exit 0
fi

install -d "$dest_root"

find "$source_root" -mindepth 1 -maxdepth 1 -type d -print0 \
    | while IFS= read -r -d '' applet_dir; do
        name="$(basename "$applet_dir")"
        manifest="$applet_dir/Cargo.toml"
        descriptor="$applet_dir/applet.toml"

        if [[ ! -f "$descriptor" ]]; then
            echo "missing applet descriptor: $descriptor" >&2
            exit 1
        fi
        if [[ ! -f "$manifest" ]]; then
            echo "missing Rust applet manifest: $manifest" >&2
            exit 1
        fi

        cargo build --release --locked --manifest-path "$manifest"

        binary="$applet_dir/target/release/$name"
        if [[ ! -x "$binary" ]]; then
            echo "missing built applet binary: $binary" >&2
            echo "Rust applet package name/binary must match directory name: $name" >&2
            exit 1
        fi

        install -Dm644 "$descriptor" "$dest_root/$name/applet.toml"
        install -Dm755 "$binary" "$dest_root/$name/$name"
    done
