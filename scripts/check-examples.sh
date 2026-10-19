#!/usr/bin/env bash
set -uo pipefail
shopt -s nullglob

status=0
found=0
for blp in var/widget_examples/*.blp; do
    found=1
    if ! out=$(blueprint-compiler compile --output /dev/null "$blp" 2>&1); then
        printf '%s\n%s\n' "$blp" "$out"
        status=1
    fi
done
if [ "$found" = 0 ]; then
    echo "no blueprints found in var/widget_examples" >&2
    exit 1
fi
exit "$status"
