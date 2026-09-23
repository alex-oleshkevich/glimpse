#!/usr/bin/env bash
set -uo pipefail

paths=("$@")
if [[ "${#paths[@]}" -eq 0 ]]; then
    shopt -s nullglob
    paths=(crates/*/blueprints/*.blp var/widget_examples/*.blp)
fi
blueprint-compiler format -f "${paths[@]}"
