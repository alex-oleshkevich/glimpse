#!/usr/bin/env bash
set -uo pipefail

report=$(blueprint-compiler lint crates/*/blueprints/*.blp 2>&1)
clean=$(printf '%s\n' "$report" | sed -e 's/\x1b\[[0-9;]*m//g')
if printf '%s\n' "$clean" | grep -E '^(warning|error)' | grep -v scrollable_parent > /dev/null; then
    printf '%s\n' "$report"
    exit 1
fi
