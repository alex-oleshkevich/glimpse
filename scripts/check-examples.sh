#!/usr/bin/env bash
set -uo pipefail
shopt -s nullglob

status=0
for blp in var/widget_examples/*.blp; do
    if ! out=$(blueprint-compiler compile --output /dev/null "$blp" 2>&1); then
        printf '%s\n%s\n' "$blp" "$out"
        status=1
    fi
done
exit "$status"
